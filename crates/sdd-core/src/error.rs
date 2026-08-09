//! SddError 是 Core 内部统一错误类型。
//!
//! 每个错误都带有稳定的错误码（E_*）、退出码以及建议的下一步命令。
//! 翻译自 早期 Node 实现。

use crate::contracts::{error_exit_codes, CommandError};

#[derive(Debug, Clone)]
pub struct SddError {
    pub code: String,
    pub message: String,
    pub next: Option<String>,
    pub exit_code: i32,
}

impl SddError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            next: None,
            exit_code: error_exit_codes(code),
        }
    }

    pub fn with_next(mut self, next: &str) -> Self {
        self.next = Some(next.to_string());
        self
    }

    pub fn to_command_error(&self) -> CommandError {
        CommandError {
            code: self.code.clone(),
            message: self.message.clone(),
            next: self.next.clone(),
        }
    }
}

impl std::fmt::Display for SddError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SddError {}
