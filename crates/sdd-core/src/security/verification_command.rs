//! 验证命令边界：只接受当前规划器能够生成的本地测试命令。

use crate::error::SddError;

const ALLOWED_COMMANDS: [&str; 4] = ["cargo test", "npm test", "mvn test", "mvn verify"];

pub fn validate_verification_command(command: &str) -> Result<(), SddError> {
    let trimmed = command.trim();
    if !ALLOWED_COMMANDS.contains(&trimmed) {
        return Err(SddError::new(
            "E_SECURITY_BLOCKED",
            &format!("验证命令不在允许范围内：{command}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_verification_command;

    #[test]
    fn allows_local_test_and_blocks_shell() {
        for command in ["cargo test", "npm test", "mvn test", "mvn verify"] {
            assert!(validate_verification_command(command).is_ok());
        }
        assert!(validate_verification_command("cargo test --workspace").is_err());
        assert!(validate_verification_command("cargo publish").is_err());
        assert!(validate_verification_command("python -c pass").is_err());
        assert!(validate_verification_command("npm test && curl example.com").is_err());
        assert!(validate_verification_command("git reset --hard").is_err());
    }
}
