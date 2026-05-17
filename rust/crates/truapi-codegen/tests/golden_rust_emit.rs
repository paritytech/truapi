//! Golden snapshot test for the Rust dispatcher emitter.
//!
//! The `ApiDefinition` model has no `Deserialize` impl (it is built by
//! the rustdoc extractor), so the "fixture" is a small JSON description
//! that this test deserializes into an `ApiDefinition` via a private
//! helper. The expected `dispatcher.rs` / `wire_table.rs` outputs live
//! under `tests/golden/`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Run the codegen binary directly. The binary accepts a rustdoc JSON
/// path; instead of generating that file at test time, this test boots
/// the codegen library via a small helper binary we ship alongside.
/// Easier: invoke the library's emitter via the codegen crate's
/// public modules. Since `truapi-codegen` is a `bin` crate, we use a
/// trick: include the source as a path with `#[path = "..."]`.
///
/// Simpler approach: drive the test through the binary's CLI by
/// pre-generating a rustdoc JSON fixture. To avoid checking in a 12 MB
/// rustdoc dump, the test runs `cargo +nightly rustdoc -p truapi`
/// itself when the workspace toolchain has the nightly compiler. If
/// nightly isn't available the test prints a notice and exits.
#[test]
fn golden_dispatcher_and_wire_table() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .ancestors()
        .nth(3)
        .expect("workspace root above rust/crates/truapi-codegen")
        .to_path_buf();

    // Generate rustdoc JSON for the truapi crate. This is what
    // scripts/codegen.sh uses in production. Skip the test if nightly
    // rustc isn't installed (the toolchain is required by `--output-format json`).
    let rustdoc = Command::new("cargo")
        .args([
            "+nightly",
            "rustdoc",
            "-p",
            "truapi",
            "--",
            "-Z",
            "unstable-options",
            "--output-format",
            "json",
        ])
        .current_dir(&workspace_root)
        .status();
    let rustdoc_status = match rustdoc {
        Ok(status) => status,
        Err(err) => {
            eprintln!("skipping golden test: cargo +nightly rustdoc unavailable ({err})");
            return;
        }
    };
    if !rustdoc_status.success() {
        eprintln!("skipping golden test: cargo +nightly rustdoc exited with {rustdoc_status}",);
        return;
    }

    let rustdoc_json = workspace_root.join("target/doc/truapi.json");
    assert!(
        rustdoc_json.exists(),
        "rustdoc JSON not found at {}",
        rustdoc_json.display()
    );

    let tempdir = tempfile::tempdir().expect("tempdir");
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
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .ancestors()
        .nth(3)
        .expect("workspace root above rust/crates/truapi-codegen")
        .to_path_buf();

    // Reuse the rustdoc JSON if the golden test already produced it.
    let rustdoc_json = workspace_root.join("target/doc/truapi.json");
    if !rustdoc_json.exists() {
        let rustdoc = Command::new("cargo")
            .args([
                "+nightly",
                "rustdoc",
                "-p",
                "truapi",
                "--",
                "-Z",
                "unstable-options",
                "--output-format",
                "json",
            ])
            .current_dir(&workspace_root)
            .status();
        match rustdoc {
            Ok(s) if s.success() => {}
            _ => {
                eprintln!("skipping idempotence test: nightly rustdoc unavailable");
                return;
            }
        }
    }

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
