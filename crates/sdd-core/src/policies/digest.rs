//! 策略摘要：SHA-256 稳定指纹。
//!
//! 语义对齐 `packages/agent-policies/src/digest.ts` 的 digest 功能：
//! 同一输入产生同一摘要；内容变化摘要必变。

/// 计算内容稳定摘要（SHA-256）
pub fn digest(content: &str) -> String {
    digest_bytes(content.as_bytes())
}

pub fn digest_bytes(content: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(content))
}
