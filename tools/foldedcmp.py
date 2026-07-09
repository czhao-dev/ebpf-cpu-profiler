#!/usr/bin/env python3
"""Compare two folded-stack files by top-N-function overlap.

Used both manually and by tools/diff_vs_perf.sh to sanity-check this
profiler's output against `perf`'s on the same workload, without
requiring byte-identical stacks - frame-pointer-only unwinding and
perf's own unwinder can legitimately resolve non-leaf frames
differently, so exact-match comparison would be too strict to be
useful.

Usage:
    foldedcmp.py FILE_A FILE_B [--top N] [--threshold T]

Exits 0 if the top-N function sets overlap (Jaccard index) at or
above T, non-zero otherwise. Prints both top-N lists and the score.
"""
import argparse
import re
import sys
from collections import defaultdict

OFFSET_RE = re.compile(r"\+0x[0-9a-fA-F]+$")


def strip_offset(frame: str) -> str:
    return OFFSET_RE.sub("", frame)


def top_functions(path: str, top_n: int) -> list:
    weights = defaultdict(int)
    with open(path) as f:
        for line in f:
            line = line.rstrip("\n")
            if not line:
                continue
            chain, _, count_s = line.rpartition(" ")
            if not chain or not count_s.isdigit():
                continue
            count = int(count_s)
            # Credit every frame in the chain, not just the leaf: a
            # function that's hot as a caller should count too, and the
            # two unwinders can disagree on exactly which frame is the
            # leaf when one truncates earlier than the other.
            for frame in set(chain.split(";")):
                name = strip_offset(frame)
                if name and name != "[unknown]":
                    weights[name] += count
    ranked = sorted(weights.items(), key=lambda kv: kv[1], reverse=True)
    return [name for name, _ in ranked[:top_n]]


def jaccard(a: list, b: list) -> float:
    sa, sb = set(a), set(b)
    if not sa and not sb:
        return 1.0
    return len(sa & sb) / len(sa | sb)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("file_a")
    parser.add_argument("file_b")
    parser.add_argument("--top", type=int, default=5)
    parser.add_argument("--threshold", type=float, default=0.6)
    args = parser.parse_args()

    top_a = top_functions(args.file_a, args.top)
    top_b = top_functions(args.file_b, args.top)
    overlap = jaccard(top_a, top_b)

    print(f"top {args.top} functions in {args.file_a}: {top_a}")
    print(f"top {args.top} functions in {args.file_b}: {top_b}")
    print(f"overlap (Jaccard): {overlap:.2f} (threshold {args.threshold:.2f})")

    if overlap < args.threshold:
        print("FAIL: overlap below threshold", file=sys.stderr)
        return 1
    print("PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
