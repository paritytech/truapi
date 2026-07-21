//! Runs a user host-script under `bun`, driving a host through the injected
//! `truapi` global.
//!
//! The Rust CLI owns the flow: it starts the host, then spawns `js/runner.ts`
//! (which connects the `@parity/truapi` client to the host and evaluates the
//! user script). The child's exit status becomes the host command's status, so
//! `truapi-host pairing-host --script foo.ts` *is* the test — there is no
//! separate bun orchestrator.

use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use anyhow::{Context, Result};
use tokio::process::Command;

/// Locate `js/runner.ts`, shipped alongside the crate.
///
/// Overridable with `TRUAPI_HOST_RUNNER` for packaged/relocated builds.
fn runner_path() -> PathBuf {
    if let Ok(path) = std::env::var("TRUAPI_HOST_RUNNER") {
        return PathBuf::from(path);
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("js/runner.ts")
}

/// Locate one of the product scripts shipped with the CLI crate.
pub fn bundled_script(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("js/scripts")
        .join(name)
}

/// Run `script` against the host serving frames at `frame_url`, as product
/// `product_id`. Inherits stdio so the script's output and any CLI confirmation
/// prompts share the terminal. Returns the child's exit status.
pub async fn run(frame_url: &str, product_id: &str, script: &Path) -> Result<ExitStatus> {
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

    Command::new("bun")
        .arg(&runner)
        .env("TRUAPI_FRAME_URL", frame_url)
        .env("TRUAPI_PRODUCT_ID", product_id)
        .env("TRUAPI_SCRIPT", &script)
        .status()
        .await
        .context("failed to spawn `bun` for the host script (is bun installed?)")
}
