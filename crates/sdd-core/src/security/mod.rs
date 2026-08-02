//! 安全层：路径安全、任务范围、密钥扫描与不可信内容边界。

pub mod secrets_scanner;
pub mod task_scope;
pub mod verification_command;

pub use task_scope::validate_file_change;
