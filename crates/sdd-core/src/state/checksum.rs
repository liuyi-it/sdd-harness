//! runtime.json 校验和。
//!
//! 校验和用于发现意外损坏；它与数据文件同目录保存，不能作为对有写权限攻击者的
//! 认证边界。因此这里使用语义准确、实现更短的 SHA-256，而不是带公开固定密钥的
//! HMAC。

use sha2::{Digest, Sha256};

/// 计算内容的 SHA-256，返回小写十六进制字符串。
pub fn compute(content: &[u8]) -> String {
    hex(&Sha256::digest(content))
}

/// 校验内容与边车中的十六进制校验和是否一致。
pub fn verify(content: &[u8], expected_hex: &str) -> bool {
    compute(content) == expected_hex.trim()
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{compute, verify};

    #[test]
    fn checksum_is_64_char_hex_and_stable() {
        let checksum = compute(b"hello");
        assert_eq!(
            checksum,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(checksum.len(), 64);
        assert!(checksum.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(checksum, compute(b"world"));
    }

    #[test]
    fn verify_accepts_matching_checksum_and_rejects_tampering() {
        let content = b"{\"schemaVersion\":5}";
        let checksum = compute(content);
        assert!(verify(content, &checksum));
        assert!(verify(content, &format!("{checksum}\n")));
        assert!(!verify(b"{\"schemaVersion\":6}", &checksum));
    }
}
