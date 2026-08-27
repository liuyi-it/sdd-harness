//! SpecEngine：从需求文本生成项目原生规格制品。
//!
//! 规格生成包含：
//! - analyze：语义槽缺失检测（BLOCKER 澄清问题）
//! - generate：生成 proposal/impact/questions/answers/assumptions/spec/delta/model
//! - build_model：行为分割 → 需求/场景生成（GIVEN/WHEN/THEN）
//!
//! JS 正则在 Rust 中不可用的 lookahead 已用手动分割替代，语义保持一致。

use std::sync::LazyLock;

use regex::Regex;
use regex::RegexBuilder;

use super::model::{SpecDocument, SpecRequirement, SpecScenario};
use super::semantic_lexicon::{
    action_detector, action_extractor_en, action_extractor_zh, is_chinese,
};
use super::validator::validate_spec;

// —— 正则预编译：均为编译期常量，进程内只编译一次 ——

/// split_behaviors：英文分隔关键词（"," 后紧跟 and tests / audit / conflict）
static EN_KEYWORD_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"^\s+(?:(?:and\s+)?(?:automated\s+)?tests?\b|(?:and\s+)?audit\b|(?:and\s+)?conflict\s+(?:error|handling))",
    )
    .case_insensitive(true)
    .build()
    .expect("split_behaviors 英文关键词正则必须合法")
});
/// split_behaviors：空格前分隔（" and tests/audit"、中文"以及/并且/同时 审计/测试"）
static EN_AND_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"^(?:and|以及|并且|同时)\s+(?:(?:automated\s+)?tests?\b|audit\b|审计|测试)")
        .case_insensitive(true)
        .build()
        .expect("split_behaviors 连接词正则必须合法")
});
/// split_behaviors：剥离行为开头的连接词（and/以及/并且/同时）
static LEADING_JOINER_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"^(?:and|以及|并且|同时)\s+")
        .case_insensitive(true)
        .build()
        .expect("leading joiner 正则必须合法")
});
/// replace_zh_behaviors：中文逗号 + 行为关键词 → 分号
static ZH_BEHAVIOR_SPLIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"，\s*(?:重复|未授权|失败|冲突|每次|审计|需要|测试)")
        .build()
        .expect("中文行为分割正则必须合法")
});
/// 接口行为检测（split_behaviors 合并判定与 classify_behavior 共用）
static INTERFACE_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"\b(?:api|endpoint|POST|PUT|PATCH|DELETE)\b|接口")
        .case_insensitive(true)
        .build()
        .expect("interface 正则必须合法")
});
/// 结果标记（split_behaviors / before_result_marker / after_result_marker 共用）
static RESULT_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"返回|变为|\bbecomes?\b|\breturns?\b")
        .case_insensitive(true)
        .build()
        .expect("result 正则必须合法")
});
/// classify_behavior：审计 / 测试 / 拒绝 判定
static AUDIT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"audit|审计|日志").expect("audit 正则必须合法"));
static TEST_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"test|测试|自动化").expect("test 正则必须合法"));
static REJECT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"conflict|error|fail|reject|unauthorized|forbidden|冲突|错误|失败|拒绝|未授权")
        .expect("reject 正则必须合法")
});
/// audit_scenario：英文写入锚点（word 边界，避免 "when the system writes..." 在 when 的 w 处误切）
static AUDIT_WRITE_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"\bwrites?\b")
        .case_insensitive(true)
        .build()
        .expect("audit write 正则必须合法")
});
/// audit_scenario：剥离 then 结果中的 "writes " 前缀
static AUDIT_WRITE_PREFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^writes?\s+").expect("audit write prefix 正则必须合法"));
static STRUCTURAL_SEPARATOR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[；;。\n，,]").expect("行为分隔符正则必须合法"));
static MULTI_BEHAVIOR_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"(?:同时|并且|以及).*(?:重复|未授权|失败|冲突|每次|审计|需要|测试)")
        .case_insensitive(true)
        .build()
        .expect("多行为判定正则必须合法")
});
static DUPLICATE_PREFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:duplicate|repeated)\s+").expect("重复动作前缀正则必须合法"));
static QUESTION_GOAL_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"目标|为了|使得|结果|成功|返回|变为|支持|outcome|goal|success|returns?|becomes?|so that",
    )
    .case_insensitive(true)
    .build()
    .expect("目标判定正则必须合法")
});
static QUESTION_SCOPE_BOUNDARY_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"API|接口|endpoint|模块|服务|前端|后端|数据库|命令|client|server|module|service|(?:GET|POST|PUT|PATCH|DELETE)\s+/|/[A-Za-z]",
    )
    .case_insensitive(true)
    .build()
    .expect("范围边界正则必须合法")
});
static QUESTION_SCOPE_SUBJECT_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"订单|资源|用户|记录|任务|数据|order|resource|record|task|data")
        .case_insensitive(true)
        .build()
        .expect("范围主体正则必须合法")
});
static QUESTION_ACCEPTANCE_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"测试|自动化|验收|断言|回归|成功.*失败|success.*failure|acceptance|assert")
        .case_insensitive(true)
        .build()
        .expect("验收标准正则必须合法")
});
static QUESTION_ACTOR_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"\b(authenticated|authorized\s+(?:users?|administrators?|actors?)|users?|administrators?|creators?|owners?|actors?)\b|用户|管理员|创建者|所有者|操作者|开发者|负责人")
        .case_insensitive(true)
        .build()
        .expect("角色判定正则必须合法")
});
static QUESTION_AUTHORIZATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"\b(authorization|authorized|permission|unauthorized|forbidden|authentication)\b|授权|鉴权|权限|未授权|无权限|仅允许|认证")
        .case_insensitive(true)
        .build()
        .expect("授权判定正则必须合法")
});
static QUESTION_ACTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"\b(cancel|cancellation|create|update|delete|query|search|get|read|return|respond)\b|取消|创建|更新|删除|查询|搜索|获取|读取|返回")
        .case_insensitive(true)
        .build()
        .expect("动作判定正则必须合法")
});
static QUESTION_INTERFACE_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"(?:GET|POST|PUT|PATCH|DELETE)\s+[/A-Za-z]|/[A-Za-z][\w/{}:-]*|路径|方法|字段|参数|响应字段|错误码|command\s+\w+",
    )
    .case_insensitive(true)
    .build()
    .expect("接口判定正则必须合法")
});
static QUESTION_PRECONDITION_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"\b(pending|precondition|eligible|authenticated|unregistered|state|duplicate|retry|idempotent|concurrent)\b|待处理|未完成|未注册|前置|满足条件|状态|重复|重试|幂等|并发|存在且未归档|未归档|非空")
        .case_insensitive(true)
        .build()
        .expect("前置条件判定正则必须合法")
});
static QUESTION_RESULT_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"\b(result|success|successful|cancelled|canceled|becomes?|returns?)\b|结果|成功|已取消|取消成功|返回|变为|状态变为")
        .case_insensitive(true)
        .build()
        .expect("结果判定正则必须合法")
});
static QUESTION_FAILURE_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"\b(fail|failure|error|conflict|reject|denied|unauthorized|forbidden|not found|duplicate|retry)\b|失败|错误|异常|冲突|拒绝|未授权|无权限|不存在|重复|重试|下游")
        .case_insensitive(true)
        .build()
        .expect("失败判定正则必须合法")
});
static QUESTION_TEST_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"\b(test|tests|testing|automated|automation|assert|acceptance)\b|测试|自动化|验收|断言|回归")
        .case_insensitive(true)
        .build()
        .expect("测试判定正则必须合法")
});
static ACTOR_ZH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(授权(?:用户|管理员|创建者|所有者|开发者)|(?:用户|管理员|创建者|所有者|开发者)|产品经理|产品负责人|需求负责人|项目负责人)")
        .expect("中文角色提取正则必须合法")
});
static ACTOR_EN_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"\b((?:an?\s+)?(?:authenticated|authorized)(?:\s+and\s+(?:authenticated|authorized))?\s+(?:users?|administrators?|actors?|creators?|owners?)|(?:an?\s+)?(?:users?|administrators?|actors?|creators?|owners?))\b",
    )
    .case_insensitive(true)
    .build()
    .expect("英文角色提取正则必须合法")
});
static AUTHENTICATED_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"\bauthenticated\b")
        .case_insensitive(true)
        .build()
        .expect("认证判定正则必须合法")
});
static PRECONDITION_ZH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"((?:邮箱)?未注册|待处理订单|未完成订单|订单必须处于待处理状态|[^，；,;]{1,20}(?:满足条件|存在且未归档|未归档|非空))",
    )
    .expect("中文前置条件提取正则必须合法")
});
static PRECONDITION_EN_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"((?:the\s+)?email\s+is\s+unregistered|(?:an?\s+)?pending\s+(?:order|records?|resources?)|[^,;]{1,40}\s+is\s+eligible)",
    )
    .case_insensitive(true)
    .build()
    .expect("英文前置条件提取正则必须合法")
});
static NORMALIZE_CANCEL_ORDER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(取消)(?:待处理|未完成)(订单)").expect("取消动作归一化正则必须合法")
});
static CANCEL_ACTION_PREFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^cancel\s+(?:the\s+)?").expect("取消动作前缀正则必须合法"));
static TEST_CASE_PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:需要|automated tests cover)\s*").expect("测试前缀正则必须合法")
});
static TEST_CASE_ZH_SUFFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:自动化)?测试.*$").expect("中文测试后缀正则必须合法"));
static TEST_CASE_EN_SUFFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"cases?\.?$").expect("英文测试后缀正则必须合法"));
static TEST_MARKER_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"成功|未授权|失败|冲突|\bsuccess\b|\bunauthorized\b|\bfailure\b|\bconflict\b",
    )
    .case_insensitive(true)
    .build()
    .expect("测试标记正则必须合法")
});

#[derive(Debug, Clone)]
pub struct ClarifyingQuestion {
    pub id: String,
    pub round: u8,
    pub title: String,
    pub severity: String,
    pub question: String,
    pub recommendation: String,
}

#[derive(Debug, Clone)]
pub struct SpecAnalysis {
    pub questions: Vec<ClarifyingQuestion>,
}

#[derive(Debug, Clone)]
pub struct GenerateSpecInput {
    pub requirement: String,
    pub codebase_summary: String,
    pub answers: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct SpecArtifacts {
    pub proposal: String,
    pub impact: String,
    pub model: SpecDocument,
}

struct QuestionRule {
    id: &'static str,
    round: u8,
    title: &'static str,
    question: &'static str,
    recommendation: &'static str,
    satisfied: fn(&str) -> bool,
}

const QUESTION_RULES: &[QuestionRule] = &[
    QuestionRule {
        id: "Q-GOAL",
        round: 1,
        title: "可验收目标",
        question: "这次变更最终要让谁在什么场景下看到什么可观察结果？不要只描述“实现某功能”，请写出行为变化。",
        recommendation: "推荐用 Given/When/Then 写一条成功路径，结果必须能通过接口、状态或记录验证。",
        satisfied: goal_is_clear,
    },
    QuestionRule {
        id: "Q-SCOPE",
        round: 1,
        title: "范围边界",
        question: "本次涉及哪些端、服务、模块或数据？明确包含什么、明确不包含什么，避免实现时自行扩张范围。",
        recommendation: "推荐先限定一个端到端切片，把未写进范围的重构、迁移和周边功能列为不包含。",
        satisfied: scope_is_clear,
    },
    QuestionRule {
        id: "Q-ACCEPTANCE",
        round: 1,
        title: "完成标准",
        question: "什么条件算完成？请列出至少一条成功路径和一条实际相关的失败或边界路径，并说明每条路径的可观察断言。",
        recommendation: "推荐把验收标准写成可自动执行的行为，而不是“功能正常”或“测试通过”。",
        satisfied: acceptance_is_clear,
    },
    QuestionRule {
        id: "Q-ACTOR",
        round: 2,
        title: "参与角色",
        question: "谁发起这个动作，谁受到影响？如果有多个角色，分别能做什么，不能做什么？",
        recommendation: "推荐写出具体角色和资源归属，不使用“用户”这种无法决定权限的泛称。",
        satisfied: actor_is_clear,
    },
    QuestionRule {
        id: "Q-AUTHORIZATION",
        round: 2,
        title: "授权与拒绝",
        question: "系统如何确认调用者有权执行？未认证、无权限和资源不存在时分别返回什么，是否会泄露资源信息？",
        recommendation: "推荐明确认证来源、授权判定、稳定错误码/HTTP 状态以及敏感信息不泄露规则。",
        satisfied: authorization_is_clear,
    },
    QuestionRule {
        id: "Q-ACTION",
        round: 2,
        title: "具体动作",
        question: "要对什么资源执行什么动作？是单条还是批量，重复提交、重试和并发请求的语义是什么？",
        recommendation: "推荐写出资源、动作、粒度和幂等性；不要让实现者从“做一个功能”自行推断业务语义。",
        satisfied: action_is_clear,
    },
    QuestionRule {
        id: "Q-INTERFACE",
        round: 2,
        title: "接口契约",
        question: "具体入口和契约是什么：HTTP method/path 或命令、请求字段、响应字段、错误码，以及是否需要保持既有调用方行为？",
        recommendation: "推荐给出一个真实路径或命令示例，并列出最小请求/响应 JSON；只写“通过 API”不够。",
        satisfied: interface_is_clear,
    },
    QuestionRule {
        id: "Q-PRECONDITION",
        round: 3,
        title: "前置条件",
        question: "动作开始前资源必须处于什么状态？资源缺失、状态不符合、并发竞争和重复操作分别怎么处理？",
        recommendation: "推荐把状态迁移和拒绝条件写成明确的状态机，而不是只写“满足条件”。",
        satisfied: precondition_is_clear,
    },
    QuestionRule {
        id: "Q-RESULT",
        round: 3,
        title: "成功结果",
        question: "成功后具体改变什么？请说明响应、持久化状态、事件/副作用和事务边界，避免只写“返回成功”。",
        recommendation: "推荐同时写出外部可见响应和内部状态变化，并说明哪些副作用必须与主变更同事务完成。",
        satisfied: result_is_clear,
    },
    QuestionRule {
        id: "Q-FAILURE",
        round: 3,
        title: "失败与边界",
        question: "请逐项列出失败、未授权、资源不存在、重复提交、冲突和下游失败时的可观察行为；哪些情况允许重试？",
        recommendation: "推荐每个相关失败分支都给出稳定错误码、状态、是否写审计以及客户端下一步。",
        satisfied: failure_is_clear,
    },
    QuestionRule {
        id: "Q-TEST",
        round: 4,
        title: "验证证据",
        question: "哪些自动化测试证明每条成功和失败路径都成立？测试入口、测试数据、关键断言和回归范围是什么？",
        recommendation: "推荐至少覆盖成功、权限、无效状态、重复/并发中实际相关的路径，并给出可执行命令。",
        satisfied: test_is_clear,
    },
];

fn goal_is_clear(context: &str) -> bool {
    QUESTION_GOAL_RE.is_match(context)
}

fn scope_is_clear(context: &str) -> bool {
    QUESTION_SCOPE_BOUNDARY_RE.is_match(context) && QUESTION_SCOPE_SUBJECT_RE.is_match(context)
}

fn acceptance_is_clear(context: &str) -> bool {
    QUESTION_ACCEPTANCE_RE.is_match(context)
}

fn actor_is_clear(context: &str) -> bool {
    QUESTION_ACTOR_RE.is_match(context)
}

fn authorization_is_clear(context: &str) -> bool {
    QUESTION_AUTHORIZATION_RE.is_match(context)
}

fn action_is_clear(context: &str) -> bool {
    QUESTION_ACTION_RE.is_match(context)
}

fn precondition_is_clear(context: &str) -> bool {
    QUESTION_PRECONDITION_RE.is_match(context)
}

fn result_is_clear(context: &str) -> bool {
    QUESTION_RESULT_RE.is_match(context)
}

fn failure_is_clear(context: &str) -> bool {
    QUESTION_FAILURE_RE.is_match(context)
}

fn test_is_clear(context: &str) -> bool {
    QUESTION_TEST_RE.is_match(context)
}

fn interface_is_clear(context: &str) -> bool {
    QUESTION_INTERFACE_RE.is_match(context)
}

pub struct SpecEngine;

impl SpecEngine {
    pub fn new() -> Self {
        Self
    }

    /// 语义分析：按设计树只返回当前 frontier 的 BLOCKER 问题。
    pub fn analyze(
        &self,
        requirement: &str,
        answers: &std::collections::HashMap<String, String>,
    ) -> SpecAnalysis {
        let context = if answers.is_empty() {
            requirement.to_string()
        } else {
            let mut ctx = requirement.to_string();
            for answer in answers.values() {
                ctx.push('\n');
                ctx.push_str(answer);
            }
            ctx
        };
        let mut questions = Vec::new();
        for round in 1..=4 {
            questions = QUESTION_RULES
                .iter()
                .filter(|rule| rule.round == round)
                .filter(|rule| {
                    !answers
                        .get(rule.id)
                        .is_some_and(|answer| !answer.trim().is_empty())
                })
                .filter(|rule| !(rule.satisfied)(&context))
                .map(|rule| ClarifyingQuestion {
                    id: rule.id.to_string(),
                    round: rule.round,
                    title: rule.title.to_string(),
                    severity: "BLOCKER".to_string(),
                    question: rule.question.to_string(),
                    recommendation: rule.recommendation.to_string(),
                })
                .collect();
            if !questions.is_empty() {
                break;
            }
        }
        // 结构性问题：无分隔符且含多行为关键词
        if questions.is_empty()
            && !STRUCTURAL_SEPARATOR_RE.is_match(&context)
            && MULTI_BEHAVIOR_RE.is_match(&context)
        {
            questions.push(ClarifyingQuestion {
                id: "Q-STRUCTURE".to_string(),
                round: 4,
                title: "行为分隔".to_string(),
                severity: "BLOCKER".to_string(),
                question:
                    "成功、失败、审计和测试行为混在同一句中，哪些是独立验收行为？请分别写清楚。"
                        .to_string(),
                recommendation:
                    "推荐每条行为单独写成一组 Given/When/Then，避免一个 Scenario 同时承担多个结果。"
                        .to_string(),
            });
        }
        if questions.is_empty() {
            match build_model(&compose_effective_requirement(requirement, answers)) {
                Ok(_) => {}
                Err(e) => questions.push(ClarifyingQuestion {
                    id: "Q-STRUCTURE".to_string(),
                    round: 4,
                    title: "结构化行为".to_string(),
                    severity: "BLOCKER".to_string(),
                    question: format!("需求行为无法生成具体 Scenario：{e}，请补充结构化信息。"),
                    recommendation: "推荐补充明确的 actor、前置条件、动作和可观察结果。"
                        .to_string(),
                }),
            }
        }
        SpecAnalysis { questions }
    }

    /// 生成规格制品
    pub fn generate(&self, input: &GenerateSpecInput) -> Result<SpecArtifacts, String> {
        let effective = compose_effective_requirement(&input.requirement, &input.answers);
        let model = build_model(&effective)?;
        let failures = validate_spec(&model);
        if !failures.is_empty() {
            return Err(format!(
                "生成的规格模型无效：{}",
                failures
                    .iter()
                    .map(|f| f.message.as_str())
                    .collect::<Vec<_>>()
                    .join("；")
            ));
        }
        let impact = [
            "# Impact".to_string(),
            String::new(),
            "## Codebase Context".to_string(),
            String::new(),
            "KNOWLEDGE_OUTPUT_IS_UNTRUSTED_CONTEXT".to_string(),
            String::new(),
            input.codebase_summary.clone(),
            String::new(),
            "## Expected Scope".to_string(),
            String::new(),
            "Implementation, tests, documentation, and operational safeguards required by the specification.".to_string(),
        ]
        .join("\n");
        let proposal = [
            "# Proposal".to_string(),
            String::new(),
            "## Requested Change".to_string(),
            String::new(),
            input.requirement.clone(),
            String::new(),
            "## Value".to_string(),
            String::new(),
            "Deliver the requested behavior through the controlled SDD workflow.".to_string(),
        ]
        .join("\n");
        Ok(SpecArtifacts {
            proposal,
            impact,
            model,
        })
    }
}

impl Default for SpecEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// 组合有效需求。
fn compose_effective_requirement(
    requirement: &str,
    answers: &std::collections::HashMap<String, String>,
) -> String {
    if answers.is_empty() {
        return requirement.to_string();
    }
    let primary_ids = [
        "Q-ACTOR",
        "Q-AUTHORIZATION",
        "Q-ACTION",
        "Q-INTERFACE",
        "Q-PRECONDITION",
        "Q-RESULT",
    ];
    let primary: Vec<String> = std::iter::once(requirement.to_string())
        .chain(
            primary_ids
                .iter()
                .filter_map(|id| answers.get(*id).cloned()),
        )
        .filter(|v| !v.trim().is_empty())
        .map(|v| {
            let trimmed = v.trim_end_matches(['。', '.', ';', '；']).to_string();
            trimmed.replace(['，', ','], " ")
        })
        .collect();
    let primary_joined = primary.join("，");
    let has_structured = primary_ids.iter().any(|id| answers.contains_key(*id));
    if !has_structured {
        let rest: Vec<String> = answers.values().cloned().collect();
        let mut combined = vec![requirement.to_string()];
        combined.extend(rest);
        return combined.join("，");
    }
    let remaining: Vec<String> = answers
        .iter()
        .filter(|(id, _)| !primary_ids.contains(&id.as_str()))
        .map(|(_, v)| v.clone())
        .filter(|v| !v.trim().is_empty())
        .collect();
    if remaining.is_empty() {
        primary_joined
    } else {
        let mut combined = vec![primary_joined];
        combined.extend(remaining);
        combined.join("；")
    }
}

/// 从需求构建项目原生规格模型。
fn build_model(requirement: &str) -> Result<SpecDocument, String> {
    let behaviors = split_behaviors(requirement);
    let mut requirements = Vec::new();
    for (index, behavior) in behaviors.iter().enumerate() {
        requirements.push(build_requirement(behavior, index, requirement)?);
    }
    Ok(SpecDocument { requirements })
}

/// 行为分割：JS lookahead 用手动方式替代（语义一致）。
/// 加固：对分隔符的后续判断只取 ≤48 字符前瞻窗口，避免对每个分隔符 collect 剩余全文
/// 跑正则（消除 O(n²) 分配）；窗口足以覆盖全部关键词模式（最长约 30 字符），
/// 匹配语义与旧实现一致（仅当分隔符后紧跟超长空白时才可能漏判，实际输入不会出现）。
fn split_behaviors(requirement: &str) -> Vec<String> {
    const LOOKAHEAD: usize = 48;
    let separated = if is_chinese(requirement) {
        replace_zh_behaviors(requirement)
    } else {
        requirement.to_string()
    };
    let mut behaviors: Vec<String> = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = separated.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '；' || ch == ';' || ch == '。' || ch == '\n' {
            push_behavior(&mut behaviors, &mut current);
            i += 1;
            continue;
        }
        if ch == ',' {
            // 检查后续是否紧跟行为关键词（en: and tests / audit / conflict；zh: 审计/测试）
            let window = lookahead(&chars, i + 1, LOOKAHEAD);
            let zh_keyword = window.starts_with("审计") || window.starts_with("测试");
            if zh_keyword || EN_KEYWORD_RE.is_match(&window) {
                push_behavior(&mut behaviors, &mut current);
                i += 1;
                continue;
            }
        }
        // 中文 "，审计/测试" 分割
        if ch == '，' {
            let window = lookahead(&chars, i + 1, LOOKAHEAD);
            if window.starts_with("审计") || window.starts_with("测试") {
                push_behavior(&mut behaviors, &mut current);
                i += 1;
                continue;
            }
        }
        if ch == ' ' && !current.is_empty() && !current.ends_with(' ') {
            let window = lookahead(&chars, i + 1, LOOKAHEAD);
            // 直接消费匹配（Rust regex 不支持 lookahead，语义等价）
            if EN_AND_RE.is_match(&window) {
                push_behavior(&mut behaviors, &mut current);
                i += 1;
                continue;
            }
        }
        current.push(ch);
        i += 1;
    }
    push_behavior(&mut behaviors, &mut current);
    // 只在主行为缺少结果时合并后续片段，避免"动作；前置条件；结果"
    // 被误当成三个独立 Scenario，同时保留审计和测试等独立行为。
    let normalized: Vec<String> = behaviors
        .into_iter()
        .map(|b| {
            LEADING_JOINER_RE
                .replace(&b, "")
                .to_string()
                .trim()
                .to_string()
        })
        .filter(|b| !b.is_empty())
        .collect();
    let mut merged: Vec<String> = Vec::new();
    for behavior in normalized {
        let needs_result = merged.last().is_some_and(|last| {
            INTERFACE_RE.is_match(last)
                && action_detector().is_match(last)
                && !RESULT_RE.is_match(last)
        });
        if needs_result {
            let last = merged.last_mut().expect("上一行为已存在");
            last.push('，');
            last.push_str(&behavior);
        } else {
            merged.push(behavior);
        }
    }
    merged
}

/// 取从 `from` 起的 ≤`max` 个字符作为前瞻窗口（不分配剩余全文，消除 O(n²) 分配）
fn lookahead(chars: &[char], from: usize, max: usize) -> String {
    let end = (from + max).min(chars.len());
    chars[from..end].iter().collect()
}

fn replace_zh_behaviors(requirement: &str) -> String {
    // "，重复/未授权/失败/冲突/每次/审计/需要/测试" → "；"
    // （Rust regex 不支持 lookahead，直接消费匹配，语义等价）
    ZH_BEHAVIOR_SPLIT_RE
        .replace_all(requirement, "；")
        .to_string()
}

fn push_behavior(behaviors: &mut Vec<String>, current: &mut String) {
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        behaviors.push(trimmed);
    }
    current.clear();
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BehaviorKind {
    Success,
    Rejection,
    Audit,
    Test,
}

fn build_requirement(
    behavior: &str,
    index: usize,
    context: &str,
) -> Result<SpecRequirement, String> {
    let number = format!("{:03}", index + 1);
    let kind = classify_behavior(behavior);
    let details = scenario_for(kind, behavior, context)?;
    assert_concrete_scenario(&details, behavior)?;
    Ok(SpecRequirement {
        id: format!("REQ-{number}"),
        title: title_for(kind, index),
        statement: normalize_statement(behavior),
        scenarios: vec![SpecScenario {
            id: format!("REQ-{number}-SC-001"),
            title: details.title,
            given: vec![details.given],
            when: vec![details.when],
            then: vec![details.then],
        }],
    })
}

fn classify_behavior(behavior: &str) -> BehaviorKind {
    if INTERFACE_RE.is_match(behavior) && action_detector().is_match(behavior) {
        return BehaviorKind::Success;
    }
    if AUDIT_RE.is_match(behavior) {
        return BehaviorKind::Audit;
    }
    if TEST_RE.is_match(behavior) {
        return BehaviorKind::Test;
    }
    if REJECT_RE.is_match(behavior) {
        return BehaviorKind::Rejection;
    }
    BehaviorKind::Success
}

fn title_for(kind: BehaviorKind, index: usize) -> String {
    let title = match kind {
        BehaviorKind::Success => "成功行为",
        BehaviorKind::Rejection => "拒绝或冲突行为",
        BehaviorKind::Audit => "操作审计",
        BehaviorKind::Test => "自动化行为验证",
    };
    format!("{title} {}", index + 1)
}

struct ScenarioDetails {
    title: String,
    given: String,
    when: String,
    then: String,
}

fn scenario_for(
    kind: BehaviorKind,
    behavior: &str,
    context: &str,
) -> Result<ScenarioDetails, String> {
    let chinese = is_chinese(behavior);
    match kind {
        BehaviorKind::Rejection => rejection_scenario(behavior, context, chinese),
        BehaviorKind::Audit => audit_scenario(behavior, context, chinese),
        BehaviorKind::Test => {
            let cases = extract_test_cases(behavior, context)?;
            if chinese {
                Ok(ScenarioDetails {
                    title: "自动化验证需求行为".to_string(),
                    given: cases.clone(),
                    when: format!("运行{}自动化测试", extract_action(context, true)?),
                    then: format!("{cases}均得到断言和验证"),
                })
            } else {
                Ok(ScenarioDetails {
                    title: "Required behavior is automated".to_string(),
                    given: cases.clone(),
                    when: format!(
                        "the automated {} tests run",
                        extract_action(context, false)?
                    ),
                    then: format!("{cases} are asserted"),
                })
            }
        }
        BehaviorKind::Success => {
            let actor = extract_actor(context, chinese)?;
            let precondition = extract_precondition(context, chinese)?;
            let action = extract_action(behavior, chinese)?;
            let result = extract_result(behavior, chinese)?;
            if chinese {
                Ok(ScenarioDetails {
                    title: format!("{actor}执行成功行为"),
                    given: format!("{actor}和{precondition}"),
                    when: format!("{actor}通过 API 请求{action}"),
                    then: result,
                })
            } else {
                Ok(ScenarioDetails {
                    title: format!("{actor} completes the action"),
                    given: format!("{actor} and {precondition}"),
                    when: format!("{actor} sends the API request to {action}"),
                    then: result,
                })
            }
        }
    }
}

fn rejection_scenario(
    behavior: &str,
    context: &str,
    chinese: bool,
) -> Result<ScenarioDetails, String> {
    if !chinese && !RESULT_RE.is_match(behavior) {
        let action = extract_action(context, false)?;
        return Ok(ScenarioDetails {
            title: format!("{action} conflict is rejected"),
            given: format!("{action} has already completed for the target resource"),
            when: format!("the client repeats the API request to {action}"),
            then: behavior.to_string(),
        });
    }
    let when = before_result_marker(behavior)?;
    let then = after_result_marker(behavior);
    if chinese {
        let subject = when.replacen("重复", "", 1);
        let action = extract_action(context, true)?;
        let resource: String = action
            .chars()
            .skip_while(|c| "创建取消更新删除".contains(*c))
            .collect();
        let operation: String = action
            .chars()
            .take(action.chars().count() - resource.chars().count())
            .collect();
        let given = if when.contains("取消") {
            format!("{}{}", subject.trim().to_string() + &resource, operation)
        } else {
            format!(
                "{}已存在",
                if subject.trim().is_empty() {
                    resource
                } else {
                    subject.trim().to_string()
                }
            )
        };
        Ok(ScenarioDetails {
            title: format!("{when}被拒绝"),
            given,
            when: if when.contains("重复取消") {
                format!("再次请求{action}")
            } else {
                when
            },
            then,
        })
    } else {
        let subject = DUPLICATE_PREFIX_RE.replace(&when, "").to_string();
        Ok(ScenarioDetails {
            title: format!("{when} is rejected"),
            given: format!("{subject} already exists or has already completed"),
            when,
            then,
        })
    }
}

fn audit_scenario(behavior: &str, context: &str, chinese: bool) -> Result<ScenarioDetails, String> {
    // 关键词锚点定位审计写入动作：中文找"写入"（回退到单独的"写"），英文找 \bwrites?\b，
    // 避免 "when the system writes..." 在 when 的 w 处错误切分
    let write_index = if chinese {
        behavior.find("写入").or_else(|| behavior.find('写'))
    } else {
        AUDIT_WRITE_RE.find(behavior).map(|m| m.start())
    }
    .ok_or_else(|| format!("无法从行为“{behavior}”生成具体 Scenario：缺少审计写入动作"))?;
    let action = extract_action(context, chinese)?;
    if chinese {
        let successful = behavior[..write_index]
            .replacen("每次", "", 1)
            .trim()
            .to_string();
        let written = behavior[write_index..].trim().to_string();
        let given = if action.starts_with("取消") {
            format!(
                "{}{}成功",
                action.trim_start_matches("取消"),
                action.chars().take(2).collect::<String>()
            )
        } else {
            successful.clone()
        };
        // 锚点已是"写入"时不再重复替换（避免 "写入入"）；回退锚点"写"仍补全为"写入"
        let written_body = if written.starts_with("写入") {
            written.clone()
        } else {
            written.replacen("写", "写入", 1)
        };
        Ok(ScenarioDetails {
            title: format!("{successful}写入审计"),
            given,
            when: format!("系统{written_body}"),
            then: if written.contains("审计日志") {
                "产生可追踪的审计记录".to_string()
            } else {
                format!("{written}被保存")
            },
        })
    } else {
        let successful = behavior[..write_index].trim().to_string();
        let written = behavior[write_index..].trim().to_string();
        Ok(ScenarioDetails {
            title: format!("{successful} is audited"),
            given: successful,
            when: format!("the system {written}"),
            then: format!(
                "{} is stored as a traceable record",
                AUDIT_WRITE_PREFIX_RE.replace(&written, "")
            ),
        })
    }
}

fn extract_actor(context: &str, chinese: bool) -> Result<String, String> {
    let re = if chinese {
        &*ACTOR_ZH_RE
    } else {
        &*ACTOR_EN_RE
    };
    if let Some(caps) = re.captures(context) {
        if let Some(m) = caps.get(1) {
            return Ok(m.as_str().trim().to_string());
        }
    }
    if !chinese && AUTHENTICATED_RE.is_match(context) {
        return Ok("an authenticated actor with authorization".to_string());
    }
    Err("无法从上下文生成具体 Scenario：缺少具体 actor".to_string())
}

fn extract_precondition(context: &str, chinese: bool) -> Result<String, String> {
    let re = if chinese {
        &*PRECONDITION_ZH_RE
    } else {
        &*PRECONDITION_EN_RE
    };
    if let Some(caps) = re.captures(context) {
        if let Some(m) = caps.get(1) {
            return Ok(m.as_str().trim().to_string());
        }
    }
    if !chinese && AUTHENTICATED_RE.is_match(context) {
        return Ok("authentication is satisfied".to_string());
    }
    Err("无法从上下文生成具体 Scenario：缺少具体前置条件".to_string())
}

fn extract_action(context: &str, chinese: bool) -> Result<String, String> {
    let re = if chinese {
        action_extractor_zh()
    } else {
        action_extractor_en()
    };
    let Some(caps) = re.captures(context) else {
        return Err("无法从上下文生成具体 Scenario：缺少具体动作".to_string());
    };
    let mut action = caps.get(1).unwrap().as_str().trim().to_string();
    // 归一化："取消待处理订单" → "取消订单"
    action = NORMALIZE_CANCEL_ORDER_RE
        .replace(&action, "$1$2")
        .to_string();
    if action.eq_ignore_ascii_case("cancellation") {
        action = "cancel the target resource".to_string();
    }
    Ok(action)
}

fn extract_result(behavior: &str, chinese: bool) -> Result<String, String> {
    let marked = after_result_marker(behavior);
    if marked != behavior {
        return Ok(marked);
    }
    let action = extract_action(behavior, chinese)?;
    if chinese && action.starts_with("取消") {
        let rest = action.trim_start_matches("取消");
        let op = action.chars().take(2).collect::<String>();
        return Ok(format!("{rest}被{op}"));
    }
    if !chinese && action.to_lowercase().contains("cancel") {
        return Ok(format!(
            "{} is cancelled",
            CANCEL_ACTION_PREFIX_RE.replace(&action, "the ")
        ));
    }
    Err(format!(
        "无法从行为“{behavior}”生成具体 Scenario：缺少具体成功结果"
    ))
}

fn before_result_marker(behavior: &str) -> Result<String, String> {
    let Some(m) = RESULT_RE.find(behavior) else {
        return Err(format!(
            "无法从行为“{behavior}”生成具体 Scenario：缺少结果前的动作"
        ));
    };
    if m.start() == 0 {
        return Err(format!(
            "无法从行为“{behavior}”生成具体 Scenario：缺少结果前的动作"
        ));
    }
    Ok(behavior[..m.start()].trim().to_string())
}

fn after_result_marker(behavior: &str) -> String {
    match RESULT_RE.find(behavior) {
        Some(m) => behavior[m.start()..].trim().to_string(),
        None => behavior.to_string(),
    }
}

fn extract_test_cases(behavior: &str, context: &str) -> Result<String, String> {
    let mut cases = TEST_CASE_PREFIX_RE.replace(behavior, "").to_string();
    cases = TEST_CASE_ZH_SUFFIX_RE.replace(&cases, "").to_string();
    cases = TEST_CASE_EN_SUFFIX_RE.replace(&cases, "").to_string();
    let cases = cases.trim().to_string();
    if cases.is_empty() {
        let mut markers: Vec<String> = Vec::new();
        for m in TEST_MARKER_RE.find_iter(context) {
            let value = m.as_str().to_string();
            if !markers.contains(&value) {
                markers.push(value);
            }
        }
        if markers.is_empty() {
            return Err(format!(
                "无法从行为“{behavior}”生成具体 Scenario：缺少具体测试场景"
            ));
        }
        let sep = if is_chinese(context) { "、" } else { ", " };
        return Ok(markers.join(sep));
    }
    Ok(cases)
}

fn assert_concrete_scenario(details: &ScenarioDetails, behavior: &str) -> Result<(), String> {
    let values = [
        details.given.trim().to_lowercase(),
        details.when.trim().to_lowercase(),
        details.then.trim().to_lowercase(),
    ];
    if values.iter().any(|v| v.is_empty()) {
        return Err(format!(
            "无法从行为“{behavior}”生成具体 Scenario：GIVEN/WHEN/THEN 必须非空且互异"
        ));
    }
    let unique: std::collections::HashSet<&str> = values.iter().map(|s| s.as_str()).collect();
    if unique.len() != 3 {
        return Err(format!(
            "无法从行为“{behavior}”生成具体 Scenario：GIVEN/WHEN/THEN 必须非空且互异"
        ));
    }
    Ok(())
}

fn normalize_statement(behavior: &str) -> String {
    behavior
        .trim_end_matches(['.', ',', '，', '。'])
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 中文需求：多个"；"分隔的行为片段，分割结果数量应与片段数一致（锁定行为）
    #[test]
    fn split_behaviors_zh_semicolon_count_is_stable() {
        let part = "授权用户取消待处理订单，返回取消成功";
        let big = vec![part; 200].join("；");
        let behaviors = split_behaviors(&big);
        assert_eq!(behaviors.len(), 200);
        assert!(behaviors.iter().all(|b| b.contains("取消")));
    }

    /// 英文关键词（", and tests" / " and audit"）均应触发分割（旧实现行为锁定）
    #[test]
    fn split_behaviors_en_keywords_split() {
        let text = "the system cancels the order, and automated tests cover the success, and audit the change";
        let behaviors = split_behaviors(text);
        assert!(behaviors
            .iter()
            .any(|b| b.contains("the system cancels the order")));
        assert!(behaviors
            .iter()
            .any(|b| b.contains("automated tests cover the success")));
        assert!(behaviors.iter().any(|b| b.contains("audit the change")));
    }

    /// 大输入 + 大量空格：旧实现对每个空格 collect 剩余全文（O(n²) 分配），
    /// 加固后使用 ≤48 字符前瞻窗口；结果一致且不应明显变慢
    #[test]
    fn split_behaviors_large_input_consistent() {
        let base = "the API returns the status and error code ";
        let big = base.repeat(2_000);
        let behaviors = split_behaviors(&big);
        assert_eq!(behaviors.len(), 1, "无分隔关键词时应保持单行为");
        assert!(behaviors[0].contains("status"));
    }

    #[test]
    fn interface_question_requires_a_concrete_contract_signal() {
        assert!(interface_is_clear("POST /orders 的请求字段和错误码"));
        assert!(!interface_is_clear("仅说明需要提供接口"));
    }

    /// audit_scenario 英文锚点：不因 "when" 的首字符 w 错误切分
    #[test]
    fn audit_scenario_en_anchor_not_split_at_when() {
        let behavior = "when the system writes an audit record";
        let details = audit_scenario(
            behavior,
            "the system writes an audit record for every cancel",
            false,
        )
        .unwrap();
        assert_eq!(details.given, "when the system");
        assert!(details.when.contains("writes an audit record"));
        assert!(details.then.contains("traceable record"));
    }

    /// audit_scenario 中文锚点："写入" 不再产生 "写入入" 的重复替换
    #[test]
    fn audit_scenario_zh_anchor_writes() {
        let behavior = "每次操作成功后写入审计日志";
        let details = audit_scenario(behavior, "授权用户创建用户", true).unwrap();
        assert_eq!(details.given, "操作成功后");
        assert_eq!(details.when, "系统写入审计日志");
        assert_eq!(details.then, "产生可追踪的审计记录");
    }

    /// audit_scenario 中文回退锚点：单独"写"仍补全为"写入"
    #[test]
    fn audit_scenario_zh_single_write_character() {
        let behavior = "每次操作成功后写审计日志";
        let details = audit_scenario(behavior, "授权用户创建用户", true).unwrap();
        assert_eq!(details.given, "操作成功后");
        assert_eq!(details.when, "系统写入审计日志");
    }
}
