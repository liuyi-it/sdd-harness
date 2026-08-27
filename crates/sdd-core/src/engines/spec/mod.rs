//! 项目原生规格引擎：需求模型、语义分析、校验与可读文档渲染。

pub mod model;
pub mod renderer;
pub mod semantic_lexicon;
pub mod spec_engine;
pub mod validator;

pub use model::{SpecDocument, SpecRequirement, SpecScenario};

use crate::error::SddError;

pub(crate) const SPEC_SCHEMA_VERSION: &str = "3.0.0";

/// 从 runtime 的 READY 规格记录读取唯一机器模型。
pub(crate) fn model_from_record(record: &serde_json::Value) -> Result<SpecDocument, SddError> {
    crate::schema::validate_json("spec", record)?;
    let model = record
        .get("model")
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "READY 规格缺少 model"))?;
    let document: SpecDocument = serde_json::from_value(model.clone()).map_err(|error| {
        SddError::new(
            "E_STATE_CORRUPTED",
            &format!("runtime.json 的规格模型无效：{error}"),
        )
    })?;
    let failures = validator::validate_spec(&document);
    if failures.is_empty() {
        Ok(document)
    } else {
        Err(SddError::new(
            "E_STATE_CORRUPTED",
            &format!(
                "runtime.json 的规格模型无效：{}",
                failures
                    .iter()
                    .map(|failure| failure.message.as_str())
                    .collect::<Vec<_>>()
                    .join("；")
            ),
        ))
    }
}
