#!/usr/bin/env bash
# Overhead benchmark: measure this profiler's (and perf's) impact on
# nginx throughput under wrk load, plus the profiler's own RSS growth
# and BPF map capacity warnings. Requires Linux, root, nginx, wrk, and
# a release build of the profiler.
#
#   tools/bench_overhead.sh [duration-seconds] [frequency-hz]
#
# Emits a markdown results table to stdout, a raw CSV
# (condition,rps,rss_peak_kb,dropped) to $BENCH_CSV_OUT (default:
# bench_overhead.csv in the current directory), and a second CSV of
# the profiler's RSS sampled once per second (<same name>_rss.csv), so
# results can be plotted later (see tools/plot_benchmark.py) without
# re-running.
set -euo pipefail

DURATION="${1:-20}"
FREQ="${2:-99}"
CSV_OUT="${BENCH_CSV_OUT:-bench_overhead.csv}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILER_BIN="$ROOT_DIR/target/release/flamegraph-profiler"
NGINX_CONF="$ROOT_DIR/tools/bench_nginx.conf"
NGINX_PORT=8080
WORKDIR="$(mktemp -d)"

nginx_ctl() {
    nginx -p "$WORKDIR" -c "$NGINX_CONF" \
        -g "pid $WORKDIR/nginx.pid; error_log $WORKDIR/logs/error.log;" "$@"
}

cleanup() {
    nginx_ctl -s stop 2>/dev/null || true
    rm -rf "$WORKDIR"
}
trap cleanup EXIT

mkdir -p "$WORKDIR/logs" "$WORKDIR/html"
echo "<html><body>ok</body></html>" > "$WORKDIR/html/index.html"

start_nginx() {
    nginx_ctl
    sleep 1
}

stop_nginx() {
    nginx_ctl -s stop 2>/dev/null || true
    sleep 1
}

run_wrk() {
    wrk -t2 -c50 -d"${DURATION}s" "http://127.0.0.1:${NGINX_PORT}/"
}

rps_from_wrk() {
    grep -oE 'Requests/sec:[[:space:]]+[0-9.]+' | awk '{print $2}'
}

degradation_pct() {
    # $1 baseline, $2 candidate
    python3 -c "b, c = $1, $2; print(f'{(1 - c / b) * 100:.1f}' if b > 0 else 'n/a')"
}

echo "condition,rps,rss_peak_kb,dropped" > "$CSV_OUT"

# --- Phase 1: baseline (no profiling) ---
start_nginx
BASELINE_OUT="$(run_wrk)"
BASELINE_RPS="$(echo "$BASELINE_OUT" | rps_from_wrk)"
stop_nginx
echo "baseline,${BASELINE_RPS},0,no" >> "$CSV_OUT"

# --- Phase 2: this profiler active, system-wide, at $FREQ Hz ---
start_nginx
"$PROFILER_BIN" record -d $((DURATION + 5)) -F "$FREQ" --format folded \
    > "$WORKDIR/profiler.folded" 2> "$WORKDIR/profiler.stderr" &
PROFILER_PID=$!
sleep 1 # let it attach before load starts

RSS_LOG="$WORKDIR/profiler_rss.log"
: > "$RSS_LOG"
(
    while kill -0 "$PROFILER_PID" 2>/dev/null; do
        awk '/VmRSS/ {print $2}' "/proc/$PROFILER_PID/status" 2>/dev/null >> "$RSS_LOG" || true
        sleep 1
    done
) &
RSS_SAMPLER_PID=$!

PROFILER_OUT="$(run_wrk)"
PROFILER_RPS="$(echo "$PROFILER_OUT" | rps_from_wrk)"

wait "$PROFILER_PID" 2>/dev/null || true
kill "$RSS_SAMPLER_PID" 2>/dev/null || true
stop_nginx

RSS_PEAK="$(sort -n "$RSS_LOG" 2>/dev/null | tail -1)"
RSS_PEAK="${RSS_PEAK:-0}"
DROPPED="no"
grep -q "near capacity" "$WORKDIR/profiler.stderr" 2>/dev/null && DROPPED="yes"
echo "profiler,${PROFILER_RPS},${RSS_PEAK},${DROPPED}" >> "$CSV_OUT"

# Persist the RSS-over-time samples too (see tools/plot_benchmark.py) -
# $RSS_LOG itself lives under $WORKDIR and is deleted on exit.
RSS_CSV_OUT="${CSV_OUT%.csv}_rss.csv"
{
    echo "sample_second,rss_kb"
    awk '{print NR","$0}' "$RSS_LOG"
} > "$RSS_CSV_OUT"

# --- Phase 3: perf active, system-wide, at $FREQ Hz ---
start_nginx
perf record -F "$FREQ" -g -a -o "$WORKDIR/perf.data" -- sleep $((DURATION + 5)) &
PERF_PID=$!
sleep 1

PERF_OUT="$(run_wrk)"
PERF_RPS="$(echo "$PERF_OUT" | rps_from_wrk)"

wait "$PERF_PID" 2>/dev/null || true
stop_nginx
echo "perf,${PERF_RPS},0,no" >> "$CSV_OUT"

# --- Markdown table ---
echo "# Overhead Benchmark Results"
echo
echo "Duration: ${DURATION}s per phase, sampling frequency: ${FREQ}Hz"
echo
echo "| Condition | RPS | Degradation vs baseline | Profiler RSS peak | Samples dropped |"
echo "|---|---|---|---|---|"
echo "| Baseline | ${BASELINE_RPS} | - | - | - |"
echo "| This profiler | ${PROFILER_RPS} | $(degradation_pct "$BASELINE_RPS" "$PROFILER_RPS")% | ${RSS_PEAK} KB | ${DROPPED} |"
echo "| perf | ${PERF_RPS} | $(degradation_pct "$BASELINE_RPS" "$PERF_RPS")% | - | - |"
echo
echo "Raw data: \`${CSV_OUT}\`"
