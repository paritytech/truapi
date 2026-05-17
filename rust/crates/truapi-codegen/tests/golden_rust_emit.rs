//! Golden snapshot test for the Rust dispatcher emitter.
//!
//! Each test runs `cargo +nightly rustdoc -p truapi` into its own
//! `--target-dir` under a per-test tempdir so concurrent test execution
//! cannot race on the shared `target/doc/truapi.json` path. Nightly Rust
//! is required; if it is not available the test panics rather than
//! silently passing (set up rustup with `rustup toolchain install nightly`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Run `cargo +nightly rustdoc -p truapi --output-format json` into the
/// given `target_dir` and return the path to the produced JSON file.
/// Panics with a clear message if nightly is unavailable so CI cannot
/// pass vacuously.
fn produce_rustdoc_json(workspace_root: &Path, target_dir: &Path) -> PathBuf {
    produce_rustdoc_json_for_package(workspace_root, target_dir, "truapi")
}

fn produce_rustdoc_json_for_package(
    workspace_root: &Path,
    target_dir: &Path,
    package: &str,
) -> PathBuf {
    let output = Command::new("cargo")
        .args(["+nightly", "rustdoc", "-p", package, "--target-dir"])
        .arg(target_dir)
        .args(["--", "-Z", "unstable-options", "--output-format", "json"])
        .current_dir(workspace_root)
        .output()
        .expect(
            "failed to spawn `cargo +nightly rustdoc`; install nightly via \
             `rustup toolchain install nightly`",
        );
    assert!(
        output.status.success(),
        "`cargo +nightly rustdoc -p {package}` failed (status {}); nightly toolchain is required.\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let json_name = package.replace('-', "_");
    let json = target_dir.join(format!("doc/{json_name}.json"));
    assert!(
        json.exists(),
        "rustdoc JSON not found at {} after successful rustdoc invocation",
        json.display(),
    );
    json
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("workspace root above rust/crates/truapi-codegen")
        .to_path_buf()
}

#[test]
fn golden_dispatcher_and_wire_table() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = workspace_root();

    let tempdir = tempfile::tempdir().expect("tempdir");
    let rustdoc_json = produce_rustdoc_json(&workspace, &tempdir.path().join("rustdoc-target"));

    let out = Command::new(env!("CARGO_BIN_EXE_truapi-codegen"))
        .args([
            "--input",
            rustdoc_json.to_str().unwrap(),
            "--output",
            tempdir.path().join("ts").to_str().unwrap(),
            "--rust-output",
            tempdir.path().join("rust").to_str().unwrap(),
        ])
        .output()
        .expect("run truapi-codegen");
    assert!(
        out.status.success(),
        "codegen failed: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // Compare both emitted files against the goldens. We assert on
    // wire_table.rs first because it's small and the diff is easy to
    // read when the wire ids drift.
    let golden_dir = manifest_dir.join("tests/golden");
    let cases = [
        ("wire_table.rs", "wire_table.rs"),
        ("dispatcher.rs", "dispatcher.rs"),
    ];
    for (golden_name, output_name) in cases {
        let golden = fs::read_to_string(golden_dir.join(golden_name))
            .unwrap_or_else(|e| panic!("read {golden_name}: {e}"));
        let actual = fs::read_to_string(tempdir.path().join("rust").join(output_name))
            .unwrap_or_else(|e| panic!("read generated {output_name}: {e}"));
        if golden != actual {
            // Dump actual to a sibling file for easy inspection
            // when running locally.
            let dump = manifest_dir.join(format!("tests/golden/{output_name}.actual"));
            let _ = fs::write(&dump, &actual);
            panic!(
                "golden mismatch for {output_name}; wrote actual to {}",
                dump.display()
            );
        }
    }
}

/// Idempotence guard at the integration level: running the binary twice
/// against the same input must produce identical output. This catches
/// non-determinism (HashMap iteration order, timestamps, etc.) that the
/// inline unit tests might miss because they exercise smaller APIs.
#[test]
fn binary_emission_is_idempotent() {
    let workspace = workspace_root();
    let tempdir = tempfile::tempdir().expect("tempdir");
    let rustdoc_json = produce_rustdoc_json(&workspace, &tempdir.path().join("rustdoc-target"));

    let run_once = || -> (String, String) {
        let tmp = tempfile::tempdir().unwrap();
        let status = Command::new(env!("CARGO_BIN_EXE_truapi-codegen"))
            .args([
                "--input",
                rustdoc_json.to_str().unwrap(),
                "--output",
                tmp.path().join("ts").to_str().unwrap(),
                "--rust-output",
                tmp.path().join("rust").to_str().unwrap(),
            ])
            .status()
            .expect("run truapi-codegen");
        assert!(status.success(), "codegen run failed");
        let dispatcher =
            fs::read_to_string(tmp.path().join("rust/dispatcher.rs")).expect("read dispatcher");
        let wire_table =
            fs::read_to_string(tmp.path().join("rust/wire_table.rs")).expect("read wire_table");
        (dispatcher, wire_table)
    };

    let (a_disp, a_wire) = run_once();
    let (b_disp, b_wire) = run_once();
    assert_eq!(a_disp, b_disp, "dispatcher.rs differs between runs");
    assert_eq!(a_wire, b_wire, "wire_table.rs differs between runs");
}

#[test]
fn golden_host_callbacks_ts() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = workspace_root();

    let tempdir = tempfile::tempdir().expect("tempdir");
    let truapi_json = produce_rustdoc_json(&workspace, &tempdir.path().join("rustdoc-target"));
    let platform_json = produce_rustdoc_json_for_package(
        &workspace,
        &tempdir.path().join("rustdoc-platform-target"),
        "truapi-platform",
    );

    let out = Command::new(env!("CARGO_BIN_EXE_truapi-codegen"))
        .args([
            "--input",
            truapi_json.to_str().unwrap(),
            "--output",
            tempdir.path().join("ts").to_str().unwrap(),
            "--platform-input",
            platform_json.to_str().unwrap(),
            "--platform-ts-output",
            tempdir.path().join("platform").to_str().unwrap(),
        ])
        .output()
        .expect("run truapi-codegen");
    assert!(
        out.status.success(),
        "codegen failed: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let golden_path = manifest_dir.join("tests/golden/host-callbacks.ts");
    let golden =
        fs::read_to_string(&golden_path).unwrap_or_else(|e| panic!("read host-callbacks.ts: {e}"));
    let actual = fs::read_to_string(tempdir.path().join("platform/host-callbacks.ts"))
        .expect("read generated host-callbacks.ts");
    if golden != actual {
        let dump = manifest_dir.join("tests/golden/host-callbacks.ts.actual");
        let _ = fs::write(&dump, &actual);
        panic!(
            "golden mismatch for host-callbacks.ts; wrote actual to {}",
            dump.display()
        );
    }
}
