//! 验证命令边界：按程序和子命令检查本地质量检查入口，参数保持独立。

use crate::error::SddError;

pub fn validate_verification_command(command: &str, args: &[String]) -> Result<(), SddError> {
    let arguments = args.iter().map(String::as_str).collect::<Vec<_>>();
    let allowed = matches!(
        (command, arguments.as_slice()),
        ("cargo", ["test" | "check" | "build" | "clippy" | "fmt", ..])
            | ("npm", ["test", ..])
            | ("npm", ["run", "test" | "lint" | "typecheck" | "build", ..])
            | ("mvn", ["test" | "verify", ..])
            | ("python" | "python3", ["-m", "unittest" | "pytest", ..])
            | ("pytest", _)
            | ("node", ["--test", ..])
    );
    let shell_syntax = std::iter::once(command)
        .chain(arguments.iter().copied())
        .any(|part| {
            part.contains(['\n', '\r', '\0', ';', '|', '&', '`', '$', '<', '>'])
                || matches!(part, "-c" | "-e" | "--eval")
        });
    if !allowed || shell_syntax {
        return Err(SddError::new(
            "E_SECURITY_BLOCKED",
            &format!(
                "验证命令不在允许范围内：{}。command 只填程序名，参数放入 args；支持 Cargo 质量检查、npm 测试/检查脚本、Maven test/verify、Python unittest/pytest 和 node --test",
                std::iter::once(command).chain(arguments.iter().copied()).collect::<Vec<_>>().join(" ")
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_verification_command;

    fn validate(command: &str, args: &[&str]) -> bool {
        validate_verification_command(
            command,
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
        .is_ok()
    }

    #[test]
    fn accepts_real_test_and_quality_commands_with_arguments() {
        for (command, args) in [
            (
                "cargo",
                vec!["test", "--workspace", "--", "--test-threads=1"],
            ),
            (
                "cargo",
                vec!["clippy", "--all-targets", "--", "-D", "warnings"],
            ),
            ("cargo", vec!["fmt", "--check"]),
            ("npm", vec!["test", "--", "--runInBand"]),
            ("npm", vec!["run", "typecheck"]),
            ("mvn", vec!["test", "-pl", "service", "-am"]),
            ("mvn", vec!["verify"]),
            ("python3", vec!["-m", "unittest", "discover", "-v"]),
            ("python", vec!["-m", "pytest", "tests/test_shipping.py"]),
            ("pytest", vec!["-q"]),
            ("node", vec!["--test", "test/shipping.test.js"]),
        ] {
            assert!(validate(command, &args), "{command} {args:?}");
        }
    }

    #[test]
    fn rejects_shell_interpreters_publication_and_combined_commands() {
        for (command, args) in [
            ("cargo test", vec![]),
            ("cargo", vec!["publish"]),
            ("npm", vec!["run", "deploy"]),
            ("sh", vec!["-c", "cargo test"]),
            ("python3", vec!["-c", "print(1)"]),
            ("python3", vec!["-m", "http.server"]),
            ("node", vec!["--eval", "1"]),
            ("npm", vec!["test", "&&", "curl"]),
            ("cargo", vec!["test", "$(whoami)"]),
            ("cargo", vec!["test", ">report.txt"]),
            ("git", vec!["reset", "--hard"]),
        ] {
            assert!(!validate(command, &args), "{command} {args:?}");
        }
    }
}
