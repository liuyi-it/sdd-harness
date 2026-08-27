//! 编译期内嵌的构建 Policy。

use super::digest::digest;

#[derive(Debug, PartialEq)]
pub struct PolicyBundle {
    pub name: &'static str,
    pub source: &'static str,
    pub digest: String,
}

const BUILTIN_POLICIES: [(&str, &str); 6] = [
    (
        "core-authority",
        include_str!("../../../../assets/policies/base/core-authority.md"),
    ),
    (
        "security-boundaries",
        include_str!("../../../../assets/policies/base/security-boundaries.md"),
    ),
    (
        "evidence-before-completion",
        include_str!("../../../../assets/policies/base/evidence-before-completion.md"),
    ),
    (
        "context-pack-consumer",
        include_str!("../../../../assets/policies/build/context-pack-consumer.md"),
    ),
    (
        "tdd-task-execution",
        include_str!("../../../../assets/policies/build/tdd-task-execution.md"),
    ),
    (
        "minimal-implementation",
        include_str!("../../../../assets/policies/shared/minimal-implementation.md"),
    ),
];

pub fn builtin_build_policies() -> &'static [PolicyBundle] {
    static POLICIES: std::sync::OnceLock<Vec<PolicyBundle>> = std::sync::OnceLock::new();
    POLICIES
        .get_or_init(|| {
            BUILTIN_POLICIES
                .iter()
                .map(|(name, content)| bundle(name, content))
                .collect()
        })
        .as_slice()
}

fn bundle(name: &'static str, content: &'static str) -> PolicyBundle {
    PolicyBundle {
        name,
        source: content,
        digest: digest(content),
    }
}
