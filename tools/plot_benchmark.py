#!/usr/bin/env python3
"""Turn tools/bench_overhead.sh's raw CSV output into charts.

Reads the two CSVs bench_overhead.sh writes (condition/rps/rss/dropped,
and a per-second RSS time series) and produces two PNGs: a grouped bar
chart of requests/sec per condition with % degradation annotated, and
a line chart of the profiler's RSS over the benchmark run. Pure
Python + matplotlib - no Linux/root needed, so this runs on whatever
machine you copied the CSVs back to.

Usage:
    plot_benchmark.py [--csv PATH] [--rss-csv PATH] [--out-dir DIR]
"""
import argparse
import csv
import sys
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

CONDITION_LABELS = {
    "baseline": "Baseline",
    "profiler": "This profiler",
    "perf": "perf",
}
CONDITION_COLORS = {
    "baseline": "#3b82c4",
    "profiler": "#8e44ad",
    "perf": "#e08030",
}


def read_rps_csv(path: Path):
    rows = {}
    with open(path, newline="") as f:
        for row in csv.DictReader(f):
            rows[row["condition"]] = row
    return rows


def read_rss_csv(path: Path):
    seconds, rss_mb = [], []
    with open(path, newline="") as f:
        for row in csv.DictReader(f):
            seconds.append(int(row["sample_second"]))
            rss_mb.append(int(row["rss_kb"]) / 1024)
    return seconds, rss_mb


def plot_rps(rows: dict, out_path: Path):
    order = [c for c in ("baseline", "profiler", "perf") if c in rows]
    labels = [CONDITION_LABELS[c] for c in order]
    values = [float(rows[c]["rps"]) for c in order]
    colors = [CONDITION_COLORS[c] for c in order]
    baseline = values[order.index("baseline")] if "baseline" in order else None

    fig, ax = plt.subplots(figsize=(6, 4.5))
    bars = ax.bar(labels, values, color=colors)
    ax.set_ylabel("Requests/sec")
    ax.set_title("nginx throughput under wrk load")

    for bar, cond, value in zip(bars, order, values):
        label = f"{value:,.0f}"
        if baseline and cond != "baseline" and baseline > 0:
            degradation = (1 - value / baseline) * 100
            label += f"\n(-{degradation:.1f}%)" if degradation > 0 else f"\n(+{-degradation:.1f}%)"
        ax.text(bar.get_x() + bar.get_width() / 2, bar.get_height(), label, ha="center", va="bottom")

    ax.set_ylim(0, max(values) * 1.2 if values else 1)
    fig.tight_layout()
    fig.savefig(out_path, dpi=150)
    plt.close(fig)


def plot_rss(seconds, rss_mb, out_path: Path):
    fig, ax = plt.subplots(figsize=(6, 4.5))
    ax.plot(seconds, rss_mb, color=CONDITION_COLORS["profiler"], marker="o", markersize=3)
    ax.set_xlabel("Time (s)")
    ax.set_ylabel("RSS (MB)")
    ax.set_title("Profiler userspace memory over the benchmark run")
    ax.set_ylim(0, max(rss_mb) * 1.3 if rss_mb else 1)
    fig.tight_layout()
    fig.savefig(out_path, dpi=150)
    plt.close(fig)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--csv", default="bench_overhead.csv")
    parser.add_argument("--rss-csv", default="bench_overhead_rss.csv")
    parser.add_argument("--out-dir", default="docs/images")
    args = parser.parse_args()

    csv_path = Path(args.csv)
    rss_csv_path = Path(args.rss_csv)
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    if not csv_path.exists():
        print(f"error: {csv_path} not found (run tools/bench_overhead.sh first)", file=sys.stderr)
        return 1

    rows = read_rps_csv(csv_path)
    rps_out = out_dir / "bench_rps.png"
    plot_rps(rows, rps_out)
    print(f"wrote {rps_out}")

    if rss_csv_path.exists():
        seconds, rss_mb = read_rss_csv(rss_csv_path)
        if seconds:
            rss_out = out_dir / "bench_rss.png"
            plot_rss(seconds, rss_mb, rss_out)
            print(f"wrote {rss_out}")
        else:
            print(f"warning: {rss_csv_path} has no samples, skipping RSS chart", file=sys.stderr)
    else:
        print(f"warning: {rss_csv_path} not found, skipping RSS chart", file=sys.stderr)

    return 0


if __name__ == "__main__":
    sys.exit(main())
