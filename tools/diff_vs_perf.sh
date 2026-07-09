#!/usr/bin/env bash
# Differential test: compare this profiler's folded-stack output
# against Linux perf's, on the same workload and sampling window, to
# sanity-check the two agree on which functions are hot. Requires
# Linux, root, `perf`, and a release build of the profiler.
#
#   tools/diff_vs_perf.sh <path-to-workload-binary> [duration-seconds]
#
# Exits non-zero if the top-5 hot functions from each side overlap
# (Jaccard) by less than 60% - loose on purpose, since frame-pointer-
# only unwinding and perf's own unwinder can legitimately disagree on
# non-leaf frames while still agreeing on what's actually hot.
set -euo pipefail

WORKLOAD_BIN="${1:?usage: diff_vs_perf.sh <workload-binary> [duration-seconds]}"
DURATION="${2:-5}"
FREQ=99

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILER_BIN="$ROOT_DIR/target/release/flamegraph-profiler"
WORKDIR="$(mktemp -d)"

"$WORKLOAD_BIN" &
WORKLOAD_PID=$!
trap 'kill "$WORKLOAD_PID" 2>/dev/null || true; rm -rf "$WORKDIR"' EXIT

echo "workload: $WORKLOAD_BIN (pid $WORKLOAD_PID), duration ${DURATION}s @ ${FREQ}Hz"

perf record -F "$FREQ" -g -p "$WORKLOAD_PID" -o "$WORKDIR/perf.data" -- sleep "$DURATION"
perf script -i "$WORKDIR/perf.data" > "$WORKDIR/perf.script"

# Collapse perf script's default output into folded-stack lines.
# Format (see `man perf-script`): one non-tab-indented summary line per
# sample, followed by tab-indented "<addr> <symbol>+<offset> (<module>)"
# frame lines leaf-first, terminated by a blank line. No vendored
# stackcollapse-perf.pl - this project avoids a Perl dependency.
awk '
/^$/ {
    if (chain != "") print chain, 1
    chain = ""
    next
}
/^\t/ {
    sym = $2
    if (sym == "") next
    chain = (chain == "") ? sym : sym ";" chain
    next
}
{ next }  # sample summary line (comm, pid, timestamp, event)
END { if (chain != "") print chain, 1 }
' "$WORKDIR/perf.script" > "$WORKDIR/perf.folded.raw"

# Sum counts for repeated identical chains.
sort "$WORKDIR/perf.folded.raw" | awk '
{
    n = NF
    count = $n
    $n = ""
    sub(/ $/, "")
    sums[$0] += count
}
END { for (chain in sums) print chain, sums[chain] }
' > "$WORKDIR/perf.folded"

"$PROFILER_BIN" record -p "$WORKLOAD_PID" -d "$DURATION" -F "$FREQ" --format folded \
    > "$WORKDIR/ours.folded"

kill "$WORKLOAD_PID" 2>/dev/null || true

echo "--- ours: $WORKDIR/ours.folded ($(wc -l < "$WORKDIR/ours.folded") stacks) ---"
echo "--- perf: $WORKDIR/perf.folded ($(wc -l < "$WORKDIR/perf.folded") stacks) ---"

python3 "$ROOT_DIR/tools/foldedcmp.py" "$WORKDIR/ours.folded" "$WORKDIR/perf.folded" --top 5 --threshold 0.6
