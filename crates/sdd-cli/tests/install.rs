//! 用真实安装脚本和受控 Cargo 子进程验证文件替换及失败语义。
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

const OLD: &str = "#!/bin/sh\necho old-version\n";

fn executable(path: &Path, content: &str) {
    std::fs::write(path, content).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for name in ["scripts", "tools", "prefix", ".sdd"] {
        std::fs::create_dir(root.join(name)).unwrap();
    }
    for name in ["install", "uninstall"] {
        std::fs::write(
            root.join(format!("scripts/{name}.sh")),
            std::fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../scripts/{name}.sh")),
            )
            .unwrap(),
        )
        .unwrap();
    }
    std::fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
    std::fs::write(root.join(".sdd/user-data"), "用户数据").unwrap();
    executable(&root.join("prefix/sdd"), OLD);
    // 只模拟构建出口；测试对象是实际脚本，而非 Rust 构建或业务验证。
    executable(
        &root.join("tools/cargo"),
        r#"#!/bin/sh
set -eu
if [ -f "$PREFIX/sdd" ]; then touch "$PWD/old-present"; fi
if [ "$BUILD_CASE" = fail ]; then exit 17; fi
build_dir="${CARGO_TARGET_DIR:-$PWD/target}"
build_target="${CARGO_BUILD_TARGET:-}"
locked=false
while [ "$#" -gt 0 ]; do
  case "$1" in
    --target-dir) build_dir="$2"; shift ;;
    --target) build_target="$2"; shift ;;
    --locked) locked=true ;;
  esac
  shift
done
echo "$locked" > "$PWD/locked"
if [ -n "$build_target" ]; then build_dir="$build_dir/$build_target"; fi
mkdir -p "$build_dir/release"
if [ "$BUILD_CASE" = invalid ]; then
  printf '#!/bin/sh\nexit 23\n' > "$build_dir/release/sdd"
else
  printf '#!/bin/sh\necho new-version\n' > "$build_dir/release/sdd"
fi
chmod +x "$build_dir/release/sdd"
"#,
    );
    dir
}

fn script(root: &Path, name: &str, case: &str) -> Output {
    let mut paths = vec![root.join("tools")];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    // rustup 的环境入口发现其 bin 已在 PATH 时不会改写前面的受控 Cargo。
    paths.push(Path::new(&std::env::var_os("HOME").unwrap()).join(".cargo/bin"));
    Command::new("bash")
        .arg(root.join(format!("scripts/{name}.sh")))
        .current_dir(root)
        .env("PREFIX", root.join("prefix"))
        .env("PATH", std::env::join_paths(paths).unwrap())
        .env("BUILD_CASE", case)
        .env("CARGO_TARGET_DIR", "custom-build")
        .env("CARGO_BUILD_TARGET", "invalid-cross-target")
        .output()
        .unwrap()
}

fn assert_no_staging(root: &Path) {
    let names: Vec<_> = std::fs::read_dir(root.join("prefix"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["sdd"]);
}

#[test]
fn failed_build_keeps_the_old_command_available() {
    let dir = project();
    let output = script(dir.path(), "install", "fail");
    assert_eq!(output.status.code(), Some(17));
    assert!(dir.path().join("old-present").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("prefix/sdd")).unwrap(),
        OLD
    );
    assert_no_staging(dir.path());
}

#[test]
fn invalid_new_binary_keeps_the_old_bytes_and_cleans_staging() {
    let dir = project();
    let output = script(dir.path(), "install", "invalid");
    assert_eq!(output.status.code(), Some(23));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("prefix/sdd")).unwrap(),
        OLD
    );
    assert_no_staging(dir.path());
}

#[test]
fn install_uses_the_locked_build_in_the_requested_directory() {
    let dir = project();
    std::fs::create_dir_all(dir.path().join("target/release")).unwrap();
    executable(
        &dir.path().join("target/release/sdd"),
        "#!/bin/sh\necho stale-version\n",
    );
    let output = script(dir.path(), "install", "valid");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(dir.path().join("old-present").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("locked"))
            .unwrap()
            .trim(),
        "true"
    );
    let installed = Command::new(dir.path().join("prefix/sdd"))
        .arg("--version")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&installed.stdout).trim(),
        "new-version"
    );
    assert_no_staging(dir.path());
}

#[test]
fn uninstall_failure_is_visible_and_does_not_report_success() {
    let dir = project();
    std::fs::remove_file(dir.path().join("prefix/sdd")).unwrap();
    std::fs::create_dir(dir.path().join("prefix/sdd")).unwrap();
    let output = script(dir.path(), "uninstall", "unused");
    assert!(!output.status.success());
    assert!(!output.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("已完整卸载"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".sdd/user-data")).unwrap(),
        "用户数据"
    );
}

#[test]
fn uninstall_removes_the_command_and_keeps_user_state() {
    let dir = project();
    let output = script(dir.path(), "uninstall", "unused");
    assert!(output.status.success());
    assert!(!dir.path().join("prefix/sdd").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".sdd/user-data")).unwrap(),
        "用户数据"
    );
}

#[test]
fn install_rejects_a_directory_destination_without_leaving_files() {
    let dir = project();
    let destination = dir.path().join("prefix/sdd");
    std::fs::remove_file(&destination).unwrap();
    std::fs::create_dir(&destination).unwrap();
    let output = script(dir.path(), "install", "valid");
    assert!(!output.status.success());
    assert_eq!(std::fs::read_dir(destination).unwrap().count(), 0);
    assert_no_staging(dir.path());
}

#[test]
fn release_example_preserves_old_bytes_until_download_and_validation_pass() {
    let guide = include_str!("../../../docs/agent-install.md");
    let example = guide
        .split("```bash\n")
        .nth(1)
        .unwrap()
        .split("\n```")
        .next()
        .unwrap();
    for case in ["download-failed", "checksum-failed", "invalid", "valid"] {
        let dir = project();
        let root = dir.path();
        std::fs::create_dir(root.join("downloads")).unwrap();
        std::fs::write(root.join("scripts/release.sh"), example).unwrap();
        executable(
            &root.join("tools/curl"),
            r#"#!/bin/sh
set -eu
if [ "$SDD_RELEASE_TEST_CASE" = download-failed ]; then exit 22; fi
destination="$3"
case "$4" in
  *.sha256)
    if [ "$SDD_RELEASE_TEST_CASE" = checksum-failed ]; then
      printf 'wrong-checksum\n' > "$destination"
    else
      shasum -a 256 "${destination%.sha256}" > "$destination"
    fi ;;
  *)
    if [ "$SDD_RELEASE_TEST_CASE" = invalid ]; then
      printf '#!/bin/sh\nexit 23\n' > "$destination"
    else
      printf '#!/bin/sh\necho new-version\n' > "$destination"
    fi ;;
esac
"#,
        );
        let output = Command::new("bash")
            .arg(root.join("scripts/release.sh"))
            .current_dir(root)
            .env("PREFIX", root.join("prefix"))
            .env("TMPDIR", root.join("downloads"))
            .env(
                "PATH",
                std::env::join_paths([
                    root.join("tools"),
                    Path::new("/usr/bin").to_path_buf(),
                    Path::new("/bin").to_path_buf(),
                ])
                .unwrap(),
            )
            .env("SDD_RELEASE_TEST_CASE", case)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            case == "valid",
            "{case}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let installed = std::fs::read_to_string(root.join("prefix/sdd")).unwrap();
        if case == "valid" {
            assert!(installed.contains("new-version"));
        } else {
            assert_eq!(installed, OLD);
        }
        assert_no_staging(root);
        assert_eq!(
            std::fs::read_dir(root.join("downloads")).unwrap().count(),
            0
        );
    }
}
