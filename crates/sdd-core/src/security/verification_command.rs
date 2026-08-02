//! 验证命令边界：只允许常见本地构建/测试入口，拒绝 shell、网络与破坏性语义。

use crate::error::SddError;

const ALLOWED_PROGRAMS: [&str; 13] = [
    "cargo",
    "npm",
    "pnpm",
    "yarn",
    "bun",
    "mvn",
    "gradle",
    "./gradlew",
    "go",
    "pytest",
    "python",
    "python3",
    "make",
];

pub fn validate_verification_command(command: &str) -> Result<(), SddError> {
    let trimmed = command.trim();
    let program = trimmed.split_whitespace().next().unwrap_or("");
    if program.is_empty()
        || !ALLOWED_PROGRAMS.contains(&program)
        || [";", "&&", "||", "|", ">", "<", "`", "$(", "\n", "\r"]
            .iter()
            .any(|token| trimmed.contains(token))
        || trimmed.split_whitespace().any(|part| {
            matches!(
                part,
                "curl" | "wget" | "ssh" | "scp" | "rm" | "sudo" | "git" | "nc" | "netcat"
            )
        })
    {
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
        assert!(validate_verification_command("cargo test --workspace").is_ok());
        assert!(validate_verification_command("npm test && curl example.com").is_err());
        assert!(validate_verification_command("git reset --hard").is_err());
    }
}
