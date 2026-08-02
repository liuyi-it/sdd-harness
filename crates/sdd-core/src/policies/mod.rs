//! 策略层：受控 Policy 的解析、编译与摘要。

pub mod compiler;
pub mod digest;
pub mod resolver;

pub use compiler::{compile_policy, rule_allows_file, PolicyRule};
pub use resolver::{builtin_build_policies, resolve_policies, PolicyBundle};
