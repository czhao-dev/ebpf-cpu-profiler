//! JIT symbolication integration test: profile a Node.js workload run
//! with `--perf-basic-prof` (which makes V8 write compiled function
//! addresses to `/tmp/perf-<pid>.map`) and assert the JIT-compiled
//! function's name - not `[unknown]`, not a raw hex address - shows up
//! in folded output.
//!
//! Requires Linux, root, a release build, and `node` on PATH. Skips
//! (rather than failing) if `node` isn't found, since Node.js isn't a
//! hard dependency of this project. Not run by default.
//!
//!   cargo build --release -p profiler
//!   sudo cargo test -p profiler --test jit -- --ignored --nocapture
#![cfg(target_os = "linux")]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
#[ignore = "requires Linux, root, a real kernel with eBPF/perf_event_open support, and node"]
fn resolves_jit_compiled_function_names() {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("skipping: `node` not found on PATH");
        return;
    }

    let root = workspace_root();
    let script = root.join("examples/jit_workload.js");

    let mut workload = Command::new("node")
        .arg("--perf-basic-prof")
        .arg(&script)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start node jit workload");

    // Give V8 time to JIT-compile hotJitFunction and write
    // /tmp/perf-<pid>.map before sampling starts.
    std::thread::sleep(Duration::from_secs(2));

    let profiler_bin = root.join("target/release/flamegraph-profiler");
    let output = Command::new(&profiler_bin)
        .args([
            "record",
            "-p",
            &workload.id().to_string(),
            "-d",
            "5",
            "--format",
            "folded",
        ])
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "failed to run {} ({e}); build it first with `cargo build --release -p profiler`",
                profiler_bin.display()
            )
        });

    workload.kill().ok();
    workload.wait().ok();

    assert!(
        output.status.success(),
        "profiler exited with an error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let folded = String::from_utf8_lossy(&output.stdout);
    assert!(
        folded.contains("hotJitFunction"),
        "expected 'hotJitFunction' (resolved via /tmp/perf-<pid>.map) to appear in a \
         sampled stack, not [unknown] or a raw address:\n{folded}"
    );
}
