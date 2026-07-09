//! Edge-case test: frame-pointer-only stack unwinding is a hardware/ABI
//! limitation, not a bug - binaries built without frame pointers
//! (`-fomit-frame-pointer`, the default on most platforms at `-O2`)
//! produce truncated call stacks, since `bpf_get_stackid` walks the
//! `rbp` chain in-kernel and has nothing to walk without one. See the
//! README's "Frame-pointer unwinding only, for now" design decision.
//!
//! Requires Linux, root, and a release build. Not run by default.
//!
//!   cargo build --release -p profiler
//!   sudo cargo test -p profiler --test edge_cases -- --ignored --nocapture
#![cfg(target_os = "linux")]

use std::path::PathBuf;
use std::process::{Command, Stdio};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
#[ignore = "requires Linux, root, and a real kernel with eBPF/perf_event_open support"]
fn omitted_frame_pointers_truncate_recursive_stacks() {
    let root = workspace_root();
    let example_src = root.join("examples/cpu_bound.c");
    let example_bin = std::env::temp_dir().join("flamegraph_profiler_cpu_bound_omit_fp_test");

    let status = Command::new("cc")
        .args(["-O2", "-fomit-frame-pointer", "-o"])
        .arg(&example_bin)
        .arg(&example_src)
        .status()
        .expect("failed to invoke cc");
    assert!(status.success(), "failed to compile examples/cpu_bound.c");

    let mut workload = Command::new(&example_bin)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start cpu_bound workload");

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

    assert!(
        output.status.success(),
        "profiler exited with an error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let folded = String::from_utf8_lossy(&output.stdout);
    assert!(folded.lines().count() > 0, "no folded stacks were produced");

    // With frame pointers present (see integration.rs), fib's recursive
    // call chain is fully recovered - a single folded line contains
    // repeated "fib" frames. Without them, bpf_get_stackid's rbp-chain
    // walk breaks after the first frame, so no chain should show two or
    // more "fib" frames.
    let deepest_fib_chain = folded
        .lines()
        .map(|line| line.matches("fib").count())
        .max()
        .unwrap_or(0);
    assert!(
        deepest_fib_chain <= 1,
        "expected omitted frame pointers to truncate the recursive fib chain \
         (at most one 'fib' frame per stack), but found a chain with {deepest_fib_chain}:\n{folded}"
    );
}
