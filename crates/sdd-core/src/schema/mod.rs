//! JSON Schema 定义与校验。

pub mod validator;

pub use validator::{schema_names, validate_json, SCHEMAS};
