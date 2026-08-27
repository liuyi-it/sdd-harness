//! 策略层：编译期内嵌 Policy 及其摘要。

pub mod digest;
pub mod resolver;

pub use resolver::{builtin_build_policies, PolicyBundle};
