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
    assert!(!String::from_utf8_lossy(&output.stdout).contains("/whoami"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("/script"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("/copy"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("/session"));
    assert!(!output.stdout.contains(&0x1b));
    assert!(!output.stderr.contains(&0x1b));
}

#[test]
fn bare_script_in_non_tty_exec_mode_fails_without_opening_an_editor() {
    let temporary = tempfile::tempdir().expect("create temporary session root");
    let output = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(temporary.path())
        .args(["exec", "/script"])
        .stdin(Stdio::null())
        .output()
        .expect("run bare script without a TTY");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("/script without a path requires an interactive terminal")
    );
    assert!(
        !temporary
            .path()
            .join("paseo-next-v2/signing-host/scripts")
            .exists()
    );
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
    assert!(first_stdout.contains("user.id=<not provisioned>"));

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

#[test]
fn existing_local_signer_is_activated_and_cached_at_startup() {
    let temporary = tempfile::tempdir().expect("create temporary session root");
    let base_path = temporary.path();
    std::fs::write(
        base_path.join("accounts.json"),
        r#"{
  "version": 1,
  "accounts": [{
    "name": "auto-1",
    "network": "paseo-next-v2",
    "mnemonic": "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    "lite_username": "cachedalice",
    "public_key_hex": "0x00",
    "address": "5GrwvaEF5zXb26Fz9rcQpDWSKfwVwqNxyvE9uZunJMtBEw2s",
    "created_at_unix": 1,
    "attested": true
  }]
}"#,
    )
    .expect("seed local account store");

    let output = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(base_path)
        .args(["exec", "/session"])
        .stdin(Stdio::null())
        .output()
        .expect("run signing-host with a cached signer");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("SIGNING_HOST_READY"));
    assert!(stdout.contains("user.id=cachedalice"));
    let metadata =
        std::fs::read_to_string(base_path.join("paseo-next-v2/signing-host/session.json"))
            .expect("read persisted session identity");
    assert!(metadata.contains("cachedalice"));
}
