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
    assert!(!output.stdout.contains(&0x1b));
    assert!(!output.stderr.contains(&0x1b));
}
