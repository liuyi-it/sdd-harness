//! OpenSpec 规格引擎（解析/渲染/模型）。

pub mod model;
pub mod parser;
pub mod renderer;
pub mod validator;

pub use model::{SpecDocument, SpecRequirement, SpecScenario};
