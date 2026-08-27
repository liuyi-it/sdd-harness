//! JSON Schema 定义与校验。

pub mod validator;

pub use validator::{validate_json, SCHEMAS};
