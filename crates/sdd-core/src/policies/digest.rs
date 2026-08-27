//! 策略摘要：SHA-256 稳定指纹。
//!
//! 为制品和状态提供稳定的 SHA-256 摘要：
//! 同一输入产生同一摘要；内容变化摘要必变。

/// 计算内容稳定摘要（SHA-256）
pub fn digest(content: &str) -> String {
    digest_bytes(content.as_bytes())
}

pub fn digest_bytes(content: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(content))
}

pub fn digest_reader(mut reader: impl std::io::Read) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};

    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::{digest_bytes, digest_reader};

    #[test]
    fn streaming_digest_matches_slice_digest() {
        let content = vec![b'x'; 128 * 1024 + 3];
        assert_eq!(
            digest_reader(std::io::Cursor::new(&content)).unwrap(),
            digest_bytes(&content)
        );
    }
}
