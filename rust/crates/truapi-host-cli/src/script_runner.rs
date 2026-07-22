//! Runs a user host-script under `bun`, driving a host through the injected
//! `truapi` global.
//!
//! The Rust CLI owns the flow: it starts the host, then spawns `js/runner.ts`
//! (which connects the `@parity/truapi` client to the host and evaluates the
//! user script). The child's exit status becomes the host command's status, so
//! `truapi-host pairing-host --script foo.ts` *is* the test — there is no
//! separate bun orchestrator.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::terminal_ui::UiHandle;

const SCRATCH_TEMPLATE: &str = r#"#!/usr/bin/env bun

// Import any npm package you need - Bun installs missing dependencies automatically.
import chalk from "chalk";

console.log(chalk.cyan.bold("\n🚀 TrUAPI script\n"));

const result = await truapi.account.getUserId();
if (!result.isOk()) {
  throw new Error(`getUserId failed: ${JSON.stringify(result.error)}`);
}
console.log(chalk.green("user id:"), result.value);
"#;

/// Locate `js/runner.ts`, shipped alongside the crate.
///
/// Overridable with `TRUAPI_HOST_RUNNER` for packaged/relocated builds.
fn runner_path() -> PathBuf {
    if let Ok(path) = std::env::var("TRUAPI_HOST_RUNNER") {
        return PathBuf::from(path);
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("js/runner.ts")
}

/// Create a durable, uniquely-named TypeScript scratch file seeded with the
/// public TrUAPI example.
pub fn create_scratch_script(directory: &Path) -> Result<PathBuf> {
    fs::create_dir_all(directory)
        .with_context(|| format!("create script directory {}", directory.display()))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for sequence in 0..100 {
        let path = directory.join(format!(
            "script-{timestamp}-{}-{sequence}.ts",
            std::process::id()
        ));
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("create scratch script {}", path.display()));
            }
        };
        file.write_all(SCRATCH_TEMPLATE.as_bytes())
            .with_context(|| format!("write scratch script {}", path.display()))?;
        return Ok(path);
    }
    anyhow::bail!(
        "could not allocate a unique scratch script in {}",
        directory.display()
    );
}

/// Open the script in the configured terminal editor and wait for it to exit.
pub async fn edit(script: &Path) -> Result<ExitStatus> {
    let specification = configured_editor();
    let (program, arguments) = parse_editor(&specification)?;
    let mut command = Command::new(program);
    command
        .args(arguments)
        .arg(script)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    command
        .status()
        .await
        .with_context(|| format!("failed to launch editor {specification:?}"))
}

fn configured_editor() -> String {
    std::env::var("VISUAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("EDITOR")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| {
            if cfg!(windows) {
                "notepad".to_string()
            } else {
                "vi".to_string()
            }
        })
}

fn parse_editor(specification: &str) -> Result<(String, Vec<String>)> {
    let mut parts = shlex::split(specification)
        .with_context(|| format!("invalid editor command {specification:?}"))?
        .into_iter();
    let program = parts
        .next()
        .with_context(|| format!("editor command is empty: {specification:?}"))?;
    Ok((program, parts.collect()))
}

/// Run `script` against the host serving frames at `frame_url`, as product
/// `product_id`. Inherits stdio so the script's output and any CLI confirmation
/// prompts share the terminal. Returns the child's exit status.
pub async fn run(frame_url: &str, product_id: &str, script: &Path) -> Result<ExitStatus> {
    command(frame_url, product_id, script)?
        .status()
        .await
        .context("failed to spawn `bun` for the host script (is bun installed?)")
}

/// Run a product script with stdout and stderr streamed into the terminal UI.
pub async fn run_captured(
    frame_url: &str,
    product_id: &str,
    script: &Path,
    ui: UiHandle,
) -> Result<ExitStatus> {
    let mut command = command(frame_url, product_id, script)?;
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .context("failed to spawn `bun` for the host script (is bun installed?)")?;
    let stdout = child.stdout.take().context("capture script stdout")?;
    let stderr = child.stderr.take().context("capture script stderr")?;
    let stdout_ui = ui.clone();
    let stdout_task = async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Some(line) = lines.next_line().await? {
            stdout_ui.script_stdout(line);
        }
        Ok::<(), std::io::Error>(())
    };
    let stderr_task = async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Some(line) = lines.next_line().await? {
            ui.script_stderr(line);
        }
        Ok::<(), std::io::Error>(())
    };
    let (status, stdout, stderr) = tokio::join!(child.wait(), stdout_task, stderr_task);
    stdout.context("read script stdout")?;
    stderr.context("read script stderr")?;
    status.context("wait for host script")
}

fn command(frame_url: &str, product_id: &str, script: &Path) -> Result<Command> {
    let runner = runner_path();
    if !runner.exists() {
        anyhow::bail!(
            "host-script runner not found at {}; set TRUAPI_HOST_RUNNER",
            runner.display()
        );
    }
    let script = script
        .canonicalize()
        .with_context(|| format!("script not found: {}", script.display()))?;

    let mut command = Command::new("bun");
    command
        .arg("run")
        .arg(&runner)
        .env("TRUAPI_FRAME_URL", frame_url)
        .env("TRUAPI_PRODUCT_ID", product_id)
        .env("TRUAPI_SCRIPT", &script);
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratch_script_starts_as_a_bun_script_with_dependency_examples() -> Result<()> {
        let temporary = tempfile::tempdir()?;

        let script = create_scratch_script(temporary.path())?;
        let contents = fs::read_to_string(script)?;

        assert!(contents.starts_with("#!/usr/bin/env bun\n"));
        assert!(contents.contains("import chalk from \"chalk\";"));
        assert!(contents.contains("truapi.account.getUserId()"));
        assert_eq!(contents, SCRATCH_TEMPLATE);
        Ok(())
    }

    #[test]
    fn host_scripts_are_run_by_bun() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let script = temporary.path().join("script.ts");
        fs::write(&script, "console.log('hello');\n")?;

        let command = command("ws://127.0.0.1:1234", "example.dot", &script)?;
        let command = command.as_std();
        let arguments = command.get_args().collect::<Vec<_>>();

        assert_eq!(command.get_program(), std::ffi::OsStr::new("bun"));
        assert_eq!(arguments[0], std::ffi::OsStr::new("run"));
        assert_eq!(arguments[1], runner_path());
        Ok(())
    }

    #[test]
    fn editor_command_accepts_quoted_arguments_without_a_shell() -> Result<()> {
        let (program, arguments) = parse_editor("code --wait \"profile one\"")?;

        assert_eq!(program, "code");
        assert_eq!(arguments, ["--wait", "profile one"]);
        Ok(())
    }
}
