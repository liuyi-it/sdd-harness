//! 策略摘要：FNV-1a 64 位稳定指纹（无外部依赖）。
//!
//! 语义对齐 `packages/agent-policies/src/digest.ts` 的 digest 功能：
//! 同一输入产生同一摘要；内容变化摘要必变。

/// 计算内容稳定摘要（FNV-1a 64）
pub fn digest(content: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in content.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
