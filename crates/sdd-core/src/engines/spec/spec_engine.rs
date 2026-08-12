//! SpecEngine：从需求文本生成 OpenSpec 规格制品。
//!
//! 翻译自 早期 Node 实现：
//! - analyze：语义槽缺失检测（BLOCKER 澄清问题）
//! - generate：生成 proposal/impact/questions/answers/assumptions/spec/delta/model
//! - build_model：行为分割 → 需求/场景生成（GIVEN/WHEN/THEN）
//!
//! JS 正则在 Rust 中不可用的 lookahead 已用手动分割替代，语义保持一致。

use regex::Regex;
use regex::RegexBuilder;

use super::semantic_lexicon::{
    action_detector, action_extractor_en, action_extractor_zh, is_chinese,
};
use crate::engines::openspec::model::{SpecDocument, SpecRequirement, SpecScenario};
use crate::engines::openspec::renderer::render_spec;
use crate::engines::openspec::validator::validate_spec;

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
    pub questions: String,
    pub answers: String,
    pub assumptions: String,
    pub spec: String,
    pub delta: String,
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
        satisfied: |context| matches(context, r"\b(authenticated|authorized\s+(?:users?|administrators?|actors?)|users?|administrators?|creators?|owners?|actors?)\b|用户|管理员|创建者|所有者|操作者|开发者|负责人"),
    },
    QuestionRule {
        id: "Q-AUTHORIZATION",
        round: 2,
        title: "授权与拒绝",
        question: "系统如何确认调用者有权执行？未认证、无权限和资源不存在时分别返回什么，是否会泄露资源信息？",
        recommendation: "推荐明确认证来源、授权判定、稳定错误码/HTTP 状态以及敏感信息不泄露规则。",
        satisfied: |context| matches(context, r"\b(authorization|authorized|permission|unauthorized|forbidden|authentication)\b|授权|鉴权|权限|未授权|无权限|仅允许|认证"),
    },
    QuestionRule {
        id: "Q-ACTION",
        round: 2,
        title: "具体动作",
        question: "要对什么资源执行什么动作？是单条还是批量，重复提交、重试和并发请求的语义是什么？",
        recommendation: "推荐写出资源、动作、粒度和幂等性；不要让实现者从“做一个功能”自行推断业务语义。",
        satisfied: |context| matches(context, r"\b(cancel|cancellation|create|update|delete|query|search|get|read|return|respond)\b|取消|创建|更新|删除|查询|搜索|获取|读取|返回"),
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
        satisfied: |context| matches(context, r"\b(pending|precondition|eligible|authenticated|unregistered|state|duplicate|retry|idempotent|concurrent)\b|待处理|未完成|未注册|前置|满足条件|状态|重复|重试|幂等|并发|存在且未归档|未归档|非空"),
    },
    QuestionRule {
        id: "Q-RESULT",
        round: 3,
        title: "成功结果",
        question: "成功后具体改变什么？请说明响应、持久化状态、事件/副作用和事务边界，避免只写“返回成功”。",
        recommendation: "推荐同时写出外部可见响应和内部状态变化，并说明哪些副作用必须与主变更同事务完成。",
        satisfied: |context| matches(context, r"\b(result|success|successful|cancelled|canceled|becomes?|returns?)\b|结果|成功|已取消|取消成功|返回|变为|状态变为"),
    },
    QuestionRule {
        id: "Q-FAILURE",
        round: 3,
        title: "失败与边界",
        question: "请逐项列出失败、未授权、资源不存在、重复提交、冲突和下游失败时的可观察行为；哪些情况允许重试？",
        recommendation: "推荐每个相关失败分支都给出稳定错误码、状态、是否写审计以及客户端下一步。",
        satisfied: |context| matches(context, r"\b(fail|failure|error|conflict|reject|denied|unauthorized|forbidden|not found|duplicate|retry)\b|失败|错误|异常|冲突|拒绝|未授权|无权限|不存在|重复|重试|下游"),
    },
    QuestionRule {
        id: "Q-TEST",
        round: 4,
        title: "验证证据",
        question: "哪些自动化测试证明每条成功和失败路径都成立？测试入口、测试数据、关键断言和回归范围是什么？",
        recommendation: "推荐至少覆盖成功、权限、无效状态、重复/并发中实际相关的路径，并给出可执行命令。",
        satisfied: |context| matches(context, r"\b(test|tests|testing|automated|automation|assert|acceptance)\b|测试|自动化|验收|断言|回归"),
    },
];

fn matches(context: &str, pattern: &str) -> bool {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .expect("question rule regex must be valid")
        .is_match(context)
}

fn goal_is_clear(context: &str) -> bool {
    matches(
        context,
        r"目标|为了|使得|结果|成功|返回|变为|支持|outcome|goal|success|returns?|becomes?|so that",
    )
}

fn scope_is_clear(context: &str) -> bool {
    let boundary = matches(
        context,
        r"API|接口|endpoint|模块|服务|前端|后端|数据库|命令|client|server|module|service|(?:GET|POST|PUT|PATCH|DELETE)\s+/|/[A-Za-z]",
    );
    let subject = matches(
        context,
        r"订单|资源|用户|记录|任务|数据|order|resource|record|task|data",
    );
    boundary && subject
}

fn acceptance_is_clear(context: &str) -> bool {
    matches(
        context,
        r"测试|自动化|验收|断言|回归|成功.*失败|success.*failure|acceptance|assert",
    )
}

fn interface_is_clear(context: &str) -> bool {
    matches(
        context,
        r"(?:GET|POST|PUT|PATCH|DELETE)\s+[/A-Za-z]|/[A-Za-z][\w/{}:-]*|路径|方法|字段|参数|响应字段|错误码|command\s+\w+",
    )
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
            && !Regex::new(r"[；;。\n，,]").unwrap().is_match(&context)
            && RegexBuilder::new(
                r"(?:同时|并且|以及).*(?:重复|未授权|失败|冲突|每次|审计|需要|测试)",
            )
            .case_insensitive(true)
            .build()
            .unwrap()
            .is_match(&context)
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
        let analysis = self.analyze(&input.requirement, &input.answers);
        let model = build_model(&effective)?;
        let failures = validate_spec(&model);
        if !failures.is_empty() {
            return Err(format!(
                "生成的 OpenSpec 无效：{}",
                failures
                    .iter()
                    .map(|f| f.message.as_str())
                    .collect::<Vec<_>>()
                    .join("；")
            ));
        }
        let spec = render_spec(&model)?;
        let questions = if analysis.questions.is_empty() {
            "# Questions\n\nNo blocker questions.".to_string()
        } else {
            let mut lines = vec!["# Questions".to_string(), String::new()];
            for q in &analysis.questions {
                lines.push(format!(
                    "## {}｜{} [{}]\n\n{}\n\n建议：{}",
                    q.id, q.title, q.severity, q.question, q.recommendation
                ));
            }
            lines.join("\n")
        };
        let answers_doc = if input.answers.is_empty() {
            "# Answers\n\nNo answers supplied.".to_string()
        } else {
            let mut lines = vec!["# Answers".to_string(), String::new()];
            for (id, answer) in &input.answers {
                lines.push(format!("## {id}\n\n{answer}"));
            }
            lines.join("\n")
        };
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
        let assumptions = [
            "# Assumptions".to_string(),
            String::new(),
            "- Existing behavior remains compatible unless explicitly changed.".to_string(),
            "- Security, audit, and tests are required for changed behavior.".to_string(),
        ]
        .join("\n");
        Ok(SpecArtifacts {
            proposal,
            impact,
            questions,
            answers: answers_doc,
            assumptions,
            spec: spec.clone(),
            delta: spec,
            model,
        })
    }
}

impl Default for SpecEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SpecEngine {
    /// 解析 spec.md（委托 OpenSpec parser）
    pub fn parse_spec_md(&self, content: &str) -> Result<SpecDocument, String> {
        crate::engines::openspec::parser::parse_spec(content)
    }

    /// 渲染 spec.md（委托 OpenSpec renderer）
    pub fn render_spec_md(&self, document: &SpecDocument) -> Result<String, String> {
        crate::engines::openspec::renderer::render_spec(document)
    }
}

/// 组合有效需求（翻译自 composeEffectiveRequirement）
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

/// 从需求构建 OpenSpec 文档（翻译自 buildModel/splitBehaviors/buildRequirement）
fn build_model(requirement: &str) -> Result<SpecDocument, String> {
    let behaviors = split_behaviors(requirement);
    let mut requirements = Vec::new();
    for (index, behavior) in behaviors.iter().enumerate() {
        requirements.push(build_requirement(behavior, index, requirement)?);
    }
    Ok(SpecDocument {
        title: "Requested Change".to_string(),
        requirements,
    })
}

/// 行为分割：JS lookahead 用手动方式替代（语义一致）
fn split_behaviors(requirement: &str) -> Vec<String> {
    let separated = if is_chinese(requirement) {
        replace_zh_behaviors(requirement)
    } else {
        requirement.to_string()
    };
    // 分割点：；;。\n 或 ", " 后紧跟测试/审计/冲突关键词
    // （正则构建一次，避免循环内重复编译）
    let en_keyword_re = RegexBuilder::new(
        r"^\s+(?:(?:and\s+)?(?:automated\s+)?tests?\b|(?:and\s+)?audit\b|(?:and\s+)?conflict\s+(?:error|handling))",
    )
    .case_insensitive(true)
    .build()
    .unwrap();
    let en_and_re = RegexBuilder::new(
        r"^(?:and|以及|并且|同时)\s+(?:(?:automated\s+)?tests?\b|audit\b|审计|测试)",
    )
    .case_insensitive(true)
    .build()
    .unwrap();
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
            let rest: String = chars[i + 1..].iter().collect();
            let zh_keyword = rest.starts_with("审计") || rest.starts_with("测试");
            if zh_keyword || en_keyword_re.is_match(&rest) {
                push_behavior(&mut behaviors, &mut current);
                i += 1;
                continue;
            }
        }
        // 中文 "，审计/测试" 与英文 " and tests/audit" 分割
        if ch == '，' {
            let rest: String = chars[i + 1..].iter().collect();
            if rest.starts_with("审计") || rest.starts_with("测试") {
                push_behavior(&mut behaviors, &mut current);
                i += 1;
                continue;
            }
        }
        if ch == ' ' && !current.is_empty() && !current.ends_with(' ') {
            let rest: String = chars[i + 1..].iter().collect();
            // 直接消费匹配（Rust regex 不支持 lookahead，语义等价）
            if en_and_re.is_match(&rest) {
                push_behavior(&mut behaviors, &mut current);
                i += 1;
                continue;
            }
        }
        current.push(ch);
        i += 1;
    }
    push_behavior(&mut behaviors, &mut current);
    // 只在主行为缺少结果时合并后续片段，避免“动作；前置条件；结果”
    // 被误当成三个独立 Scenario，同时保留审计和测试等独立行为。
    let normalized: Vec<String> = behaviors
        .into_iter()
        .map(|b| {
            let re = RegexBuilder::new(r"^(?:and|以及|并且|同时)\s+")
                .case_insensitive(true)
                .build()
                .unwrap();
            re.replace(&b, "").to_string().trim().to_string()
        })
        .filter(|b| !b.is_empty())
        .collect();
    let interface_re = RegexBuilder::new(r"\b(?:api|endpoint|POST|PUT|PATCH|DELETE)\b|接口")
        .case_insensitive(true)
        .build()
        .unwrap();
    let result_re = RegexBuilder::new(r"返回|变为|\bbecomes?\b|\breturns?\b")
        .case_insensitive(true)
        .build()
        .unwrap();
    let detector = action_detector();
    let mut merged: Vec<String> = Vec::new();
    for behavior in normalized {
        let needs_result = merged.last().is_some_and(|last| {
            interface_re.is_match(last) && detector.is_match(last) && !result_re.is_match(last)
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

fn replace_zh_behaviors(requirement: &str) -> String {
    // "，重复/未授权/失败/冲突/每次/审计/需要/测试" → "；"
    // （Rust regex 不支持 lookahead，直接消费匹配，语义等价）
    let re = RegexBuilder::new(r"，\s*(?:重复|未授权|失败|冲突|每次|审计|需要|测试)")
        .build()
        .unwrap();
    re.replace_all(requirement, "；").to_string()
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
        statement: format!("The system SHALL {}.", normalize_statement(behavior)),
        operation: "ADDED".to_string(),
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
    let interface = RegexBuilder::new(r"\b(?:api|endpoint|POST|PUT|PATCH|DELETE)\b|接口")
        .case_insensitive(true)
        .build()
        .unwrap();
    if interface.is_match(behavior) && action_detector().is_match(behavior) {
        return BehaviorKind::Success;
    }
    if Regex::new(r"audit|审计|日志").unwrap().is_match(behavior) {
        return BehaviorKind::Audit;
    }
    if Regex::new(r"test|测试|自动化").unwrap().is_match(behavior) {
        return BehaviorKind::Test;
    }
    if Regex::new(r"conflict|error|fail|reject|unauthorized|forbidden|冲突|错误|失败|拒绝|未授权")
        .unwrap()
        .is_match(behavior)
    {
        return BehaviorKind::Rejection;
    }
    BehaviorKind::Success
}

fn title_for(kind: BehaviorKind, index: usize) -> String {
    let title = match kind {
        BehaviorKind::Success => "Successful API behavior",
        BehaviorKind::Rejection => "Rejected or conflicting request",
        BehaviorKind::Audit => "Successful operation audit",
        BehaviorKind::Test => "Automated behavior verification",
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
    if !chinese
        && !RegexBuilder::new(r"\b(?:returns?|becomes?)\b")
            .case_insensitive(true)
            .build()
            .unwrap()
            .is_match(behavior)
    {
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
        let subject = Regex::new(r"^(?:duplicate|repeated)\s+")
            .unwrap()
            .replace(&when, "")
            .to_string();
        Ok(ScenarioDetails {
            title: format!("{when} is rejected"),
            given: format!("{subject} already exists or has already completed"),
            when,
            then,
        })
    }
}

fn audit_scenario(behavior: &str, context: &str, chinese: bool) -> Result<ScenarioDetails, String> {
    let write_index = behavior
        .find(['写', 'w', 'W'])
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
        Ok(ScenarioDetails {
            title: format!("{successful}写入审计"),
            given,
            when: format!("系统{}", written.replacen("写", "写入", 1)),
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
                Regex::new(r"^writes?\s+").unwrap().replace(&written, "")
            ),
        })
    }
}

fn extract_actor(context: &str, chinese: bool) -> Result<String, String> {
    let re = if chinese {
        Regex::new(r"(授权(?:用户|管理员|创建者|所有者|开发者)|(?:用户|管理员|创建者|所有者|开发者)|产品经理|产品负责人|需求负责人|项目负责人)").unwrap()
    } else {
        RegexBuilder::new(
            r"\b((?:an?\s+)?(?:authenticated|authorized)(?:\s+and\s+(?:authenticated|authorized))?\s+(?:users?|administrators?|actors?|creators?|owners?)|(?:an?\s+)?(?:users?|administrators?|actors?|creators?|owners?))\b",
        )
        .case_insensitive(true)
        .build()
        .unwrap()
    };
    if let Some(caps) = re.captures(context) {
        if let Some(m) = caps.get(1) {
            return Ok(m.as_str().trim().to_string());
        }
    }
    if !chinese
        && RegexBuilder::new(r"\bauthenticated\b")
            .case_insensitive(true)
            .build()
            .unwrap()
            .is_match(context)
    {
        return Ok("an authenticated actor with authorization".to_string());
    }
    Err("无法从上下文生成具体 Scenario：缺少具体 actor".to_string())
}

fn extract_precondition(context: &str, chinese: bool) -> Result<String, String> {
    let re = if chinese {
        Regex::new(
            r"((?:邮箱)?未注册|待处理订单|未完成订单|订单必须处于待处理状态|[^，；,;]{1,20}(?:满足条件|存在且未归档|未归档|非空))",
        )
        .unwrap()
    } else {
        RegexBuilder::new(
            r"((?:the\s+)?email\s+is\s+unregistered|(?:an?\s+)?pending\s+(?:order|records?|resources?)|[^,;]{1,40}\s+is\s+eligible)",
        )
        .case_insensitive(true)
        .build()
        .unwrap()
    };
    if let Some(caps) = re.captures(context) {
        if let Some(m) = caps.get(1) {
            return Ok(m.as_str().trim().to_string());
        }
    }
    if !chinese
        && RegexBuilder::new(r"\bauthenticated\b")
            .case_insensitive(true)
            .build()
            .unwrap()
            .is_match(context)
    {
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
    action = Regex::new(r"(取消)(?:待处理|未完成)(订单)")
        .unwrap()
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
            Regex::new(r"^cancel\s+(?:the\s+)?")
                .unwrap()
                .replace(&action, "the ")
        ));
    }
    Err(format!(
        "无法从行为“{behavior}”生成具体 Scenario：缺少具体成功结果"
    ))
}

fn before_result_marker(behavior: &str) -> Result<String, String> {
    let re = RegexBuilder::new(r"返回|变为|\bbecomes?\b|\breturns?\b")
        .case_insensitive(true)
        .build()
        .unwrap();
    let Some(m) = re.find(behavior) else {
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
    let re = RegexBuilder::new(r"返回|变为|\bbecomes?\b|\breturns?\b")
        .case_insensitive(true)
        .build()
        .unwrap();
    match re.find(behavior) {
        Some(m) => behavior[m.start()..].trim().to_string(),
        None => behavior.to_string(),
    }
}

fn extract_test_cases(behavior: &str, context: &str) -> Result<String, String> {
    let mut cases = Regex::new(r"^(?:需要|automated tests cover)\s*")
        .unwrap()
        .replace(behavior, "")
        .to_string();
    cases = Regex::new(r"(?:自动化)?测试.*$")
        .unwrap()
        .replace(&cases, "")
        .to_string();
    cases = Regex::new(r"cases?\.?$")
        .unwrap()
        .replace(&cases, "")
        .to_string();
    let cases = cases.trim().to_string();
    if cases.is_empty() {
        let markers_re = RegexBuilder::new(
            r"成功|未授权|失败|冲突|\bsuccess\b|\bunauthorized\b|\bfailure\b|\bconflict\b",
        )
        .case_insensitive(true)
        .build()
        .unwrap();
        let mut markers: Vec<String> = Vec::new();
        for m in markers_re.find_iter(context) {
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
