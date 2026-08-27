//! TddEngine：设计文档生成与任务计划生成。
//!
//! TDD 设计与计划生成：拼接当前规格对应的设计文档，并经受控规划器生成原子任务链。

use super::planner::{build_plan_artifacts, extract_paths};
use super::{PlanArtifacts, PlanningInput};
use crate::engines::spec::renderer::render_spec;
use crate::engines::spec::SpecDocument;
use crate::error::SddError;

pub struct DesignInput<'a> {
    pub specification: &'a SpecDocument,
    pub impact: &'a str,
    pub codebase_context: &'a str,
}

pub struct TddEngine;

impl TddEngine {
    pub fn new() -> Self {
        Self
    }

    /// 生成当前设计文档的固定章节结构。
    pub fn generate_design(&self, input: &DesignInput<'_>) -> Result<String, SddError> {
        let affected_files =
            extract_paths(&format!("{}\n{}", input.impact, input.codebase_context));
        let requirement_lines = structured_requirement_lines(input.specification);
        let specification = render_spec(input.specification).map_err(|error| {
            SddError::new("E_STATE_CORRUPTED", &format!("规格模型无法渲染：{error}"))
        })?;
        let affected_modules = if affected_files.is_empty() {
            "- 未从索引上下文解析到具体路径。".to_string()
        } else {
            affected_files
                .iter()
                .map(|file| format!("- {file}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        Ok([
            "# Design",
            "",
            "## Current Code Structure",
            "",
            input.codebase_context,
            "",
            "## Structured Requirements and Scenarios",
            "",
            &requirement_lines,
            "",
            "## Target Design",
            "",
            "沿用已索引代码库的现有模块边界，以每个 Requirement 的 Scenario 作为可验证行为单元。",
            "",
            "## Affected Modules and Files",
            "",
            &affected_modules,
            "",
            "## API Changes",
            "",
            "仅公开规格明确要求的接口与行为；删除被新设计替代的旧接口和兼容层。",
            "",
            "## Interfaces and Contracts",
            "",
            "模块间只通过上述公开接口交换规格所需数据；输入、输出与稳定错误均以 Scenario 为契约。",
            "",
            "## Data Changes",
            "",
            "仅持久化规格要求的状态；若涉及结构变更，需提供迁移和回滚验证。",
            "",
            "## Transaction and Idempotency",
            "",
            "状态修改保持原子性，并为规格中的重复操作定义稳定结果。",
            "",
            "## Error Handling",
            "",
            "按 Scenario 的失败路径返回稳定错误，不吞掉边界异常。",
            "",
            "## Logging and Monitoring",
            "",
            "记录必要状态变化，不记录密钥或完整源码内容。",
            "",
            "## Testing Strategy",
            "",
            "每个 Scenario 执行 RED、GREEN、REFACTOR、VERIFY 四阶段链。",
            "",
            "## Test Seams",
            "",
            "优先在公开 API 或模块导出边界建立稳定测试 seam，不依赖私有实现细节。",
            "",
            "## Risks and Rollback",
            "",
            "风险由受影响文件、状态变更和迁移路径决定；代码与数据变更应可共同回滚。",
            "",
            "## Specification Reference",
            "",
            &specification,
            "",
            "## Impact Reference",
            "",
            input.impact,
        ]
        .join("\n"))
    }

    /// 生成计划、任务文档与测试计划制品。
    pub fn generate_plan(&self, input: &PlanningInput<'_>) -> Result<PlanArtifacts, SddError> {
        build_plan_artifacts(input)
    }
}

impl Default for TddEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn structured_requirement_lines(specification: &SpecDocument) -> String {
    specification
        .requirements
        .iter()
        .flat_map(|requirement| {
            std::iter::once(format!("- {}：{}", requirement.id, requirement.title)).chain(
                requirement
                    .scenarios
                    .iter()
                    .map(|scenario| format!("  - {}：{}", scenario.id, scenario.title)),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
