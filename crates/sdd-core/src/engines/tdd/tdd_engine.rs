//! TddEngine：设计文档生成与任务计划生成。
//!
//! 翻译自 早期 Node 实现：
//! - generateDesign：拼接设计提示（含既有设计保留规则）
//! - generatePlan：经受控任务规划器生成原子任务链

use super::super::openspec::parser::parse_spec;
use super::super::superpowers::planner::{build_plan_artifacts, extract_paths};
use super::super::superpowers::protocol::PlanArtifacts;
use crate::error::SddError;

pub struct DesignInput {
    pub spec: String,
    pub impact: String,
    pub codebase_summary: String,
    pub package_structure: String,
    pub architecture: String,
    pub existing_design: Option<String>,
}

pub struct PlanningInputRust {
    pub spec: String,
    pub design: String,
    pub impact: String,
    pub codebase_summary: String,
}

pub struct TddEngine;

impl TddEngine {
    pub fn new() -> Self {
        Self
    }

    /// 生成设计文档（对齐 generateDesign 的章节结构）
    pub fn generate_design(&self, input: &DesignInput) -> String {
        let affected_files = extract_paths(&format!(
            "{}\n{}\n{}",
            input.impact, input.codebase_summary, input.architecture
        ));
        let requirement_lines = structured_requirement_lines(&input.spec);
        let mut prompt = [
            "# Design".to_string(),
            String::new(),
            "## Phase Policy".to_string(),
            String::new(),
            String::new(),
            "## Current Code Structure".to_string(),
            String::new(),
            input.codebase_summary.clone(),
            String::new(),
            input.package_structure.clone(),
            String::new(),
            "## Structured Requirements and Scenarios".to_string(),
            String::new(),
            requirement_lines.join("\n"),
            String::new(),
            "## Target Design".to_string(),
            String::new(),
            "沿用已索引代码库的现有模块边界，以每个 Requirement 的 Scenario 作为可验证行为单元。"
                .to_string(),
            String::new(),
            "## Affected Modules and Files".to_string(),
            String::new(),
            if affected_files.is_empty() {
                input.architecture.clone()
            } else {
                affected_files
                    .iter()
                    .map(|file| format!("- {file}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            },
            String::new(),
            input.architecture.clone(),
            String::new(),
            "## API Changes".to_string(),
            String::new(),
            "仅公开规格明确要求的接口行为，并保持未涉及行为兼容。".to_string(),
            String::new(),
            "## Interfaces and Contracts".to_string(),
            String::new(),
            "模块间只通过上述公开接口交换规格所需数据；输入、输出与稳定错误均以 Scenario 为契约。"
                .to_string(),
            String::new(),
            "## Data Changes".to_string(),
            String::new(),
            "仅持久化规格要求的状态；若涉及结构变更，需提供迁移和回滚验证。".to_string(),
            String::new(),
            "## Transaction and Idempotency".to_string(),
            String::new(),
            "状态修改保持原子性，并为规格中的重复操作定义稳定结果。".to_string(),
            String::new(),
            "## Error Handling".to_string(),
            String::new(),
            "按 Scenario 的失败路径返回稳定错误，不吞掉边界异常。".to_string(),
            String::new(),
            "## Logging and Monitoring".to_string(),
            String::new(),
            "记录必要状态变化，不记录密钥或完整源码内容。".to_string(),
            String::new(),
            "## Testing Strategy".to_string(),
            String::new(),
            "每个 Scenario 执行 RED、GREEN、REFACTOR、VERIFY 四阶段链。".to_string(),
            String::new(),
            "## Test Seams".to_string(),
            String::new(),
            "优先在公开 API 或模块导出边界建立稳定测试 seam，不依赖私有实现细节。".to_string(),
            String::new(),
            "## Risks and Rollback".to_string(),
            String::new(),
            "风险由受影响文件、兼容边界和状态变更决定；代码与数据变更应可共同回滚。".to_string(),
            String::new(),
            "## Specification Reference".to_string(),
            String::new(),
            input.spec.clone(),
            String::new(),
            "## Impact Reference".to_string(),
            String::new(),
            input.impact.clone(),
        ]
        .join("\n");

        if let Some(existing) = &input.existing_design {
            prompt += &format!(
                "\n\n## 已有设计文档\n\n以下是上次生成的设计文档。用户可能已在其上做了修改。请在已有内容基础上更新设计，遵循以下规则：\n1. 保留用户新增或修改的内容（手动添加的需求分析、约束、取舍说明等）。\n2. 仅因 spec/impact 变更而需要调整的部分才更新。\n3. 输出完整的设计文档，不要输出 diff 或标记变更。\n\n{existing}"
            );
        }
        prompt
    }

    /// 生成计划制品（对齐 generatePlan）
    pub fn generate_plan(&self, input: &PlanningInputRust) -> Result<PlanArtifacts, SddError> {
        let planner_input = super::super::superpowers::protocol::PlanningInput {
            spec: input.spec.clone(),
            design: input.design.clone(),
            impact: input.impact.clone(),
            codebase_summary: input.codebase_summary.clone(),
        };
        build_plan_artifacts(&planner_input)
    }

    /// 读取已有 spec.md 并解析为需求模型（供 plan 使用）
    pub fn parse_spec_md(
        &self,
        spec: &str,
    ) -> Result<super::super::openspec::model::SpecDocument, String> {
        parse_spec(spec)
    }
}

impl Default for TddEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn structured_requirement_lines(spec: &str) -> Vec<String> {
    let lines: Vec<String> = spec
        .split('\n')
        .filter(|line| {
            line.starts_with("### Requirement:")
                || line.starts_with("### REQ-")
                || line.starts_with("#### Scenario:")
        })
        .map(|line| format!("- {}", line.trim_start_matches(['#', ' '])))
        .collect();
    if lines.is_empty() {
        vec!["- 规格未包含结构化 Requirement。".to_string()]
    } else {
        lines
    }
}
