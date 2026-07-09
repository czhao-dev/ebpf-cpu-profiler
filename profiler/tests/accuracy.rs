//! Accuracy test: profile `examples/burn.c`, a workload with a known,
//! fixed 70/30 CPU-time split between two distinctly named functions,
//! and assert the profiler's sampled ratio and call hierarchy match
//! that ground truth.
//!
//! Requires Linux, root (perf_event_open + bpf()), and a release build of
//! the profiler. Not run by default - `cargo test` skips `#[ignore]`d
//! tests. Run explicitly with:
//!
//!   cargo build --release -p profiler
//!   sudo cargo test -p profiler --test accuracy -- --ignored --nocapture
#![cfg(target_os = "linux")]

use std::path::PathBuf;
use std::process::{Command, Stdio};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
#[ignore = "requires Linux, root, and a real kernel with eBPF/perf_event_open support"]
fn sampled_ratio_matches_burn_workload_split() {
    let root = workspace_root();
    let example_src = root.join("examples/burn.c");
    let example_bin = std::env::temp_dir().join("flamegraph_profiler_burn_test");

    let status = Command::new("cc")
        .args([
            "-O2",
            "-fno-omit-frame-pointer",
            "-fno-optimize-sibling-calls",
            "-o",
        ])
        .arg(&example_bin)
        .arg(&example_src)
        .status()
        .expect("failed to invoke cc");
    assert!(status.success(), "failed to compile examples/burn.c");

    let mut workload = Command::new(&example_bin)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start burn workload");

    let profiler_bin = root.join("target/release/flamegraph-profiler");
    let output = Command::new(&profiler_bin)
        .args([
            "record",
            "-p",
            &workload.id().to_string(),
            "-d",
            "8",
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

    // Sum sample counts per bucket, and separately confirm the full
    // main -> {hot_seventy,cold_thirty} -> spin call chain shows up
    // (not just the leaf function name in isolation).
    let (mut hot, mut cold) = (0u64, 0u64);
    let mut hot_chain_ok = false;
    let mut cold_chain_ok = false;
    for line in folded.lines() {
        let Some((chain, count)) = line.rsplit_once(' ') else {
            continue;
        };
        let Ok(count) = count.parse::<u64>() else {
            continue;
        };
        if chain.contains("hot_seventy") {
            hot += count;
            hot_chain_ok |= chain.contains("main") && chain.contains("spin");
        }
        if chain.contains("cold_thirty") {
            cold += count;
            cold_chain_ok |= chain.contains("main") && chain.contains("spin");
        }
    }

    assert!(hot > 0, "no samples landed in hot_seventy:\n{folded}");
    assert!(cold > 0, "no samples landed in cold_thirty:\n{folded}");
    assert!(
        hot_chain_ok,
        "expected a full main -> hot_seventy -> spin call chain:\n{folded}"
    );
    assert!(
        cold_chain_ok,
        "expected a full main -> cold_thirty -> spin call chain:\n{folded}"
    );

    // Loose tolerance (ground truth is 70%): sampling is statistical and
    // this runs on noisy shared CI/VM hardware, so a tight bound would be
    // flaky rather than meaningful.
    let hot_fraction = hot as f64 / (hot + cold) as f64;
    assert!(
        (0.62..=0.78).contains(&hot_fraction),
        "expected hot_seventy to account for ~70% of samples (62-78% tolerance), \
         got {:.1}% (hot={hot}, cold={cold}):\n{folded}",
        hot_fraction * 100.0
    );
}
