// A CPU-bound busy loop calling a distinctly named function very
// frequently, so V8 JIT-compiles it well before a profiler's sampling
// window ends. Run with --perf-basic-prof, V8 writes each JIT-compiled
// function's address range to /tmp/perf-<pid>.map - the file
// profiler/src/jitsym.rs parses as a fallback when ELF symbol
// resolution finds nothing (JIT code lives in an anonymous mapping,
// not a real file on disk).
//
//   node --perf-basic-prof examples/jit_workload.js
"use strict";

function hotJitFunction(n) {
	let acc = 0;
	for (let i = 0; i < n; i++) {
		acc += i * i;
	}
	return acc;
}

let total = 0;
for (;;) {
	total += hotJitFunction(1000000);
	if (total < 0) console.log(total); // unreachable; defeats dead-code elimination
}
