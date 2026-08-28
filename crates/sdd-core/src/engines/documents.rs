//! Agent 生成的规格、设计和计划模型。Core 只做确定性校验、渲染和持久化。

use serde::{Deserialize, Serialize};

use crate::engines::spec::SpecDocument;
use crate::engines::tdd::TaskDefinition;
use crate::error::SddError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scope {
    pub included: Vec<String>,
    pub excluded: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpecPhaseResult {
    pub schema_version: String,
    pub goal: String,
    pub scope: Scope,
    pub constraints: Vec<String>,
    pub model: SpecDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesignDecision {
    pub title: String,
    pub decision: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesignPhaseResult {
    pub schema_version: String,
    pub summary: String,
    pub current_state: Vec<String>,
    pub decisions: Vec<DesignDecision>,
    pub affected_files: Vec<String>,
    pub interfaces: Vec<String>,
    pub data_changes: Vec<String>,
    pub error_handling: Vec<String>,
    pub test_strategy: Vec<String>,
    pub risks: Vec<String>,
    pub rollback: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DependencyDecision {
    pub name: String,
    pub manifest: String,
    pub action: String,
    pub reason: String,
    pub requirements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanPhaseResult {
    pub schema_version: String,
    pub summary: String,
    pub global_constraints: Vec<String>,
    pub dependencies: Vec<DependencyDecision>,
    pub tasks: Vec<TaskDefinition>,
}

pub fn parse_spec(raw: &serde_json::Value) -> Result<SpecPhaseResult, SddError> {
    parse("spec-result", raw)
}

pub fn parse_design(raw: &serde_json::Value) -> Result<DesignPhaseResult, SddError> {
    parse("design-result", raw)
}

pub fn parse_plan(raw: &serde_json::Value) -> Result<PlanPhaseResult, SddError> {
    parse("plan-result", raw)
}

fn parse<T>(schema: &str, raw: &serde_json::Value) -> Result<T, SddError>
where
    T: for<'de> Deserialize<'de>,
{
    crate::schema::validate_json(schema, raw)
        .map_err(|error| SddError::new("E_INVALID_PHASE_COMMAND", &error.message))?;
    reject_placeholders(raw)?;
    serde_json::from_value(raw.clone()).map_err(|error| {
        SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("Agent 阶段结果解析失败：{error}"),
        )
    })
}

fn reject_placeholders(value: &serde_json::Value) -> Result<(), SddError> {
    match value {
        serde_json::Value::String(text) => {
            let upper = text.to_ascii_uppercase();
            if ["TODO", "TBD", "<PLACEHOLDER>", "待补充", "稍后填写"]
                .iter()
                .any(|marker| upper.contains(marker))
            {
                return Err(SddError::new(
                    "E_INVALID_PHASE_COMMAND",
                    "Agent 阶段结果包含占位内容",
                ));
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                reject_placeholders(value)?;
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                reject_placeholders(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn render_spec(requirement: &str, result: &SpecPhaseResult) -> Result<String, SddError> {
    let requirements = crate::engines::spec::renderer::render_spec(&result.model)
        .map_err(|error| SddError::new("E_STATE_CORRUPTED", &error))?;
    Ok(format!(
        "# 需求规格\n\n## 原始需求\n\n> {}\n\n## 目标\n\n{}\n\n## 包含范围\n\n{}\n\n## 排除范围\n\n{}\n\n## 约束与不变量\n\n{}\n\n## 需求与验收场景\n\n{}",
        requirement.replace('\n', "\n> "),
        result.goal,
        bullets(&result.scope.included),
        bullets(&result.scope.excluded),
        bullets(&result.constraints),
        requirements
    ))
}

pub fn render_design(result: &DesignPhaseResult) -> String {
    let decisions = result
        .decisions
        .iter()
        .map(|item| {
            format!(
                "### {}\n\n- 决策：{}\n- 理由：{}",
                item.title, item.decision, item.rationale
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        "# 技术设计\n\n## 方案摘要\n\n{}\n\n## 当前代码事实\n\n{}\n\n## 关键决策与取舍\n\n{}\n\n## 影响文件\n\n{}\n\n## 接口与数据流\n\n{}\n\n## 数据变化\n\n{}\n\n## 错误处理\n\n{}\n\n## 测试策略\n\n{}\n\n## 风险\n\n{}\n\n## 回滚\n\n{}\n",
        result.summary,
        bullets(&result.current_state),
        decisions,
        bullets(&result.affected_files),
        bullets(&result.interfaces),
        bullets_or_none(&result.data_changes),
        bullets(&result.error_handling),
        bullets(&result.test_strategy),
        bullets(&result.risks),
        bullets(&result.rollback),
    )
}

pub fn render_plan(result: &PlanPhaseResult) -> String {
    let dependencies = result
        .dependencies
        .iter()
        .map(|item| format!("- {}（{}）：{}", item.name, item.action, item.reason))
        .collect::<Vec<_>>();
    format!(
        "# 实施计划\n\n## 摘要\n\n{}\n\n## 全局约束\n\n{}\n\n## 实施顺序\n\n按 tasks.md 的依赖图逐个完成可独立验收的纵向任务。\n\n## 依赖决策\n\n{}\n",
        result.summary,
        bullets(&result.global_constraints),
        if dependencies.is_empty() {
            "- 无新增、升级或删除依赖。".to_string()
        } else {
            dependencies.join("\n")
        }
    )
}

pub fn render_tasks(tasks: &[TaskDefinition]) -> String {
    let mut sections = vec!["# 开发任务".to_string()];
    for task in tasks {
        let steps = task
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| format!("{}. [{}] {}", index + 1, step.kind, step.instruction))
            .collect::<Vec<_>>()
            .join("\n");
        let verification = task
            .verification
            .iter()
            .map(|item| {
                let command = std::iter::once(item.command.as_str())
                    .chain(item.args.iter().map(String::as_str))
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("- `{command}`：{}", item.expected)
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!(
            "## [ ] {}：{}\n\n- 执行模式：{}\n- 用户可见结果：{}\n- 关联需求：{}\n- 关联场景：{}\n- 前置任务：{}\n\n### 文件范围\n\n- 允许修改：{}\n- 预期新增：{}\n- 禁止修改：{}\n- 测试 seam：{}\n\n### 接口\n\n- 消费：{}\n- 产出：{}\n\n### 实施步骤\n\n{}\n\n### 验证\n\n{}\n\n### 完成标准\n\n{}\n\n### 验收标准\n\n{}",
            task.id,
            task.title,
            task.execution_mode,
            task.user_visible_outcome,
            task.requirements.join("、"),
            task.scenarios.join("、"),
            inline_or_none(&task.depends_on),
            task.allowed_files.join("、"),
            inline_or_none(&task.expected_new_files),
            task.forbidden_files.join("、"),
            task.test_seam,
            inline_or_none(&task.interfaces.consumes),
            inline_or_none(&task.interfaces.produces),
            steps,
            verification,
            bullets(&task.done_criteria),
            bullets(&task.acceptance_criteria),
        ));
    }
    sections.join("\n\n") + "\n"
}

fn bullets(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("- {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn bullets_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "- 无。".to_string()
    } else {
        bullets(values)
    }
}

fn inline_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "无".to_string()
    } else {
        values.join("、")
    }
}
