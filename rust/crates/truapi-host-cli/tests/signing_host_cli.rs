//! Process-boundary smoke tests for signing-host invocation modes.

use std::process::{Command, Stdio};

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_truapi-host"))
}

#[test]
fn interactive_mode_rejects_non_tty_stdio_with_usage_exit() {
    let output = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .stdin(Stdio::null())
        .output()
        .expect("run signing-host without a TTY");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("interactive signing-host requires a TTY")
    );
    assert!(!output.stdout.contains(&0x1b));
}

#[test]
fn exec_help_is_plain_and_exits_successfully() {
    let base_path =
        std::env::temp_dir().join(format!("truapi-host-cli-exec-help-{}", std::process::id()));
    let output = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(&base_path)
        .args(["exec", "/help"])
        .stdin(Stdio::null())
        .output()
        .expect("run signing-host exec /help");
    let _ = std::fs::remove_dir_all(base_path);

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("/whoami"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("/copy"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("/session"));
    assert!(!output.stdout.contains(&0x1b));
    assert!(!output.stderr.contains(&0x1b));
}

#[test]
fn startup_session_is_reported_and_restored() {
    let temporary = tempfile::tempdir().expect("create temporary session root");
    let base_path = temporary.path();
    let first = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(base_path)
        .args(["--session", "alice", "exec", "/session"])
        .stdin(Stdio::null())
        .output()
        .expect("run signing-host in alice session");
    assert!(first.status.success());
    let first_stdout = String::from_utf8_lossy(&first.stdout);
    assert!(first_stdout.contains("name=alice"));
    assert!(first_stdout.contains("signing-host/sessions/alice"));
    assert!(first_stdout.contains("user.id=<not activated>"));

    let restored = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(base_path)
        .args(["exec", "/session --list"])
        .stdin(Stdio::null())
        .output()
        .expect("restore signing-host session");
    assert!(restored.status.success());
    let restored_stdout = String::from_utf8_lossy(&restored.stdout);
    assert!(restored_stdout.contains("* alice"));
    assert!(restored_stdout.contains("  default"));
    assert!(!restored.stdout.contains(&0x1b));
}
