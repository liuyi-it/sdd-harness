//! 验证命令边界：按程序和子命令检查本地质量检查入口，参数保持独立。

use crate::error::SddError;

pub fn validate_verification_command(command: &str, args: &[String]) -> Result<(), SddError> {
    let arguments = args.iter().map(String::as_str).collect::<Vec<_>>();
    let allowed = matches!(
        (command, arguments.as_slice()),
        ("cargo", ["test" | "check" | "build" | "clippy" | "fmt", ..])
            | ("npm", ["test", ..])
            | ("npm", ["run", "test" | "lint" | "typecheck" | "build", ..])
            | ("python" | "python3", ["-m", "unittest" | "pytest", ..])
            | ("pytest", _)
            | ("node", ["--test", ..])
    ) || (command == "mvn" && maven_quality_arguments(&arguments));
    let shell_syntax = std::iter::once(command)
        .chain(arguments.iter().copied())
        .any(|part| {
            part.contains(['\n', '\r', '\0', ';', '|', '&', '`', '$', '<', '>'])
                || part == "--eval"
                || (command != "mvn" && matches!(part, "-c" | "-e"))
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

fn maven_quality_arguments(args: &[&str]) -> bool {
    // Maven 的选项可以放在生命周期前后；带值选项的值不是额外目标。
    const FLAGS: &[&str] = &[
        "-B",
        "--batch-mode",
        "-q",
        "--quiet",
        "-e",
        "--errors",
        "-o",
        "--offline",
        "-U",
        "--update-snapshots",
        "-N",
        "--non-recursive",
        "-am",
        "--also-make",
        "-amd",
        "--also-make-dependents",
        "-fae",
        "--fail-at-end",
        "-ff",
        "--fail-fast",
        "-nsu",
        "--no-snapshot-updates",
        "-ntp",
        "--no-transfer-progress",
        "-C",
        "--strict-checksums",
        "-c",
        "--lax-checksums",
        "-V",
        "--show-version",
        "-X",
        "--debug",
        "-itr",
        "--ignore-transitive-repositories",
    ];
    const VALUES: &[&str] = &[
        "-s",
        "--settings",
        "-gs",
        "--global-settings",
        "-t",
        "--toolchains",
        "-gt",
        "--global-toolchains",
        "-f",
        "--file",
        "-pl",
        "--projects",
        "-P",
        "--activate-profiles",
        "-rf",
        "--resume-from",
        "-T",
        "--threads",
        "-l",
        "--log-file",
        "-D",
        "--define",
        "-b",
        "--builder",
    ];
    let mut quality_goal = false;
    let mut index = 0;
    let mut options = true;
    while let Some(argument) = args.get(index).copied() {
        index += 1;
        match argument {
            "-fn" | "--fail-never" => return false,
            "test" | "verify" => quality_goal = true,
            "clean" => {}
            "--" if options => options = false,
            "--color" if options => {
                // --color 的值可省略；后续非选项仍会被 Maven 消费为颜色值。
                if let Some(value) = args.get(index).filter(|value| !value.starts_with('-')) {
                    if !matches!(*value, "auto" | "always" | "never") {
                        return false;
                    }
                    index += 1;
                }
            }
            _ if options && argument.starts_with("--color=") => {
                if !matches!(
                    argument.trim_start_matches("--color="),
                    "auto" | "always" | "never"
                ) {
                    return false;
                }
            }
            _ if options && FLAGS.contains(&argument) => {}
            _ if options && VALUES.contains(&argument) => {
                let Some(value) = args.get(index) else {
                    return false;
                };
                if value.is_empty() || value.starts_with('-') {
                    return false;
                }
                index += 1;
            }
            _ if options
                && VALUES.iter().any(|option| {
                    argument.strip_prefix(option).is_some_and(|value| {
                        if option.starts_with("--") {
                            value
                                .strip_prefix('=')
                                .is_some_and(|value| !value.is_empty())
                        } else {
                            !value.trim_start_matches('=').is_empty()
                        }
                    })
                }) => {}
            // install/deploy 和 plugin:goal 均不是质量入口；未知选项也不猜测其值。
            _ => return false,
        }
    }
    quality_goal
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
            (
                "mvn",
                vec!["-B", "-q", "-s", "settings.xml", "verify", "-e"],
            ),
            (
                "mvn",
                vec![
                    "--settings=settings.xml",
                    "-DskipTests=false",
                    "clean",
                    "test",
                ],
            ),
            ("mvn", vec!["-plservice", "-am", "-T1C", "verify"]),
            ("mvn", vec!["test", "-D", "profile=test", "-c"]),
            ("mvn", vec!["--", "clean", "verify"]),
            ("mvn", vec!["verify", "--color"]),
            ("mvn", vec!["--color", "never", "verify"]),
            ("mvn", vec!["--color=auto", "verify"]),
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
            ("mvn", vec!["test", "deploy"]),
            ("mvn", vec!["verify", "install"]),
            ("mvn", vec!["-B", "verify", "exec:java"]),
            ("mvn", vec!["--help", "test"]),
            ("mvn", vec!["-fn", "test"]),
            ("mvn", vec!["--unknown", "deploy", "test"]),
            ("mvn", vec!["--settings", "verify"]),
            ("mvn", vec!["-s", "-q", "verify"]),
            ("mvn", vec!["--settings=", "verify"]),
            ("mvn", vec!["-q", "clean"]),
            ("mvn", vec!["--", "-q", "verify"]),
            ("mvn", vec!["--color", "verify"]),
            ("mvn", vec!["--color=invalid", "verify"]),
            ("mvn", vec!["-Bq", "verify"]),
        ] {
            assert!(!validate(command, &args), "{command} {args:?}");
        }
    }
}
