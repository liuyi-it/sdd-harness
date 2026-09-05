//! JSON Schema 定义与校验。

pub mod validator;

pub use validator::{schema_source, schema_value, validate_json, SCHEMAS};
