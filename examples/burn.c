/*
 * A deterministic CPU workload with a fixed, known 70/30 time split
 * between two distinctly named functions - ground truth for checking a
 * profiler's sampled ratio against a known answer.
 *
 * hot_seventy() and cold_thirty() are identical-cost spin loops
 * (`UNIT_ITERS` each); main() calls hot_seventy() seven times and
 * cold_thirty() three times per round, repeating forever. Each round is
 * short relative to a multi-second sampling window, so both functions
 * stay on-CPU throughout the run rather than one dominating an early or
 * late slice of it.
 *
 *   cc -O2 -fno-omit-frame-pointer -o burn burn.c
 *
 * Then, in another terminal:
 *
 *   sudo flamegraph-profiler record -p $(pgrep burn) -d 8 --format=folded
 */
#include <stdio.h>

#define UNIT_ITERS 20000000UL

__attribute__((noinline)) static void spin(unsigned long iters)
{
	volatile unsigned long i;
	for (i = 0; i < iters; i++)
		;
}

__attribute__((noinline)) static void hot_seventy(void)
{
	spin(UNIT_ITERS);
}

__attribute__((noinline)) static void cold_thirty(void)
{
	spin(UNIT_ITERS);
}

int main(void)
{
	for (;;) {
		for (int i = 0; i < 7; i++)
			hot_seventy();
		for (int i = 0; i < 3; i++)
			cold_thirty();
	}
	return 0;
}
