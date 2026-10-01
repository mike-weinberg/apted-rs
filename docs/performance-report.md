# Performance report

`apted` against the upstream Java APTED, on nine 1,000-node tree pairs.
Methodology: [benchmarking.md](benchmarking.md). Raw data:
[`bench/results/x86-64-vm-run1`](../bench/results/x86-64-vm-run1) and
[`bench/results/x86-64-vm-run2`](../bench/results/x86-64-vm-run2).

## Summary

- Total time is 1.13-1.16x lower than Java's over two independent runs.
- Rust is 1.2-2.1x faster on the cheap shapes (`random`, `left`, `right`,
  `binary`, `flat`) and on par on the inner-path shapes (`zigzag-zigzag`,
  `left-right`, 1.00-1.01x), which make up 93% of Java's total.
- A literal port of the Java code, a line-by-line translation kept for
  comparison, is 0.79-0.81x as fast as Java. The implementation techniques
  described under [Why it is fast](#why-it-is-fast) make `apted` 1.43x faster
  than that literal port.
- All implementations return identical distances on all nine pairs.

## Environment

A cloud virtual machine on a shared host. Absolute numbers carry the host's
noise; the interleaved comparison is the result.

| | |
|---|---|
| CPU | Intel Xeon Processor @ 2.10 GHz (as reported by the hypervisor), 4 vCPUs, 1 thread per core, AVX2 and AVX-512 |
| Cache | L1d 48 KiB/core, L2 2 MiB/core, L3 260 MiB shared (host) |
| Memory | 15 GiB |
| Virtualization | KVM (full); frequency governor not exposed |
| OS | Ubuntu 24.04.4 LTS, Linux 6.18.44 |
| Load | 1-minute load average 0.1-0.9 at start; no other workloads during the runs |
| Rust | rustc 1.94.1 (LLVM 21.1.8), default release profile, x86_64-unknown-linux-gnu |
| Java | OpenJDK 21.0.10 (Ubuntu build), `-Xss64m -Xmx8g`, default GC and JIT |
| Reference | upstream Java APTED at commit `193666b` |

Full capture: [`env.txt`](../bench/results/x86-64-vm-run2/env.txt).

## Method

Nine tree pairs, 1,000 nodes per tree, 20 distinct single-character labels,
fixed seed, unit cost model. Every implementation reads the same trees.
Each implementation times each pair 3 times and reports the median. The
whole set runs 5 times, interleaved, and the report keeps the best median.
Java gets 2 untimed warm-up computations per pair first.

## Results

Mean of the two runs' best medians, in milliseconds (lower is better). The
columns after `literal port` add one technique each, cumulatively; the last
column is `apted`.

| Pair | Java | literal port | + unified `spf_l`/`spf_r` | + strategy matrix | + label hash | + split rows (`apted`) |
|---|---:|---:|---:|---:|---:|---:|
| random-random | 241 | 232 | 181 | 185 | 151 | 151 |
| left-left | 56 | 39 | 32 | 32 | 27 | 27 |
| right-right | 38 | 39 | 32 | 32 | 28 | 28 |
| binary-binary | 250 | 354 | 265 | 253 | 204 | 202 |
| zigzag-zigzag | 1,960 | 2,176 | 2,188 | 2,031 | 1,924 | 1,944 |
| flat-flat | 42 | 47 | 40 | 38 | 33 | 36 |
| left-right | 1,628 | 1,865 | 1,813 | 1,719 | 1,615 | 1,634 |
| zigzag-binary | 2,731 | 3,701 | 2,937 | 2,673 | 2,275 | 2,074 |
| random-zigzag | 1,593 | 2,185 | 1,750 | 1,672 | 1,369 | 1,347 |

### Totals

| | Java | literal port | + unified `spf_l`/`spf_r` | + strategy matrix | + label hash | + split rows (`apted`) |
|---|---:|---:|---:|---:|---:|---:|
| Total, run 1 (ms) | 8,678 | 10,669 | 9,369 | 8,616 | 7,612 | 7,482 |
| Total, run 2 (ms) | 8,400 | 10,608 | 9,106 | 8,654 | 7,639 | 7,405 |
| **vs Java** | 1.00x | 0.79-0.81x | 0.92-0.93x | 0.97-1.01x | 1.10-1.14x | **1.13-1.16x** |
| vs literal port | | 1.00x | 1.14-1.16x | 1.23-1.24x | 1.39-1.40x | **1.43x** |
| Change from the previous column | | | -13.2% | -6.5% | -11.7% | -2.4% |

### Speedup over Java by pair

Mean of the two runs.

| Pair | literal port | `apted` |
|---|---:|---:|
| left-left | 1.42x | 2.07x |
| random-random | 1.04x | 1.60x |
| right-right | 0.98x | 1.38x |
| zigzag-binary | 0.74x | 1.32x |
| binary-binary | 0.71x | 1.23x |
| random-zigzag | 0.73x | 1.18x |
| flat-flat | 0.89x | 1.16x |
| zigzag-zigzag | 0.90x | 1.01x |
| left-right | 0.87x | 1.00x |

## Why it is fast

The literal port mirrors the Java data structures: nested `Vec<Vec<f32>>`
tables, a bounds check on every access, and a string comparison for every
rename test. HotSpot removes many of the equivalent checks after profiling
and inlines the cost model call, so a direct translation has no inherent
advantage over Java. The techniques below account for the 1.43x gain.

- **Precomputed index arrays.** `NodeIndexer` computes the traversal, leaf,
  keyroot and size arrays once per tree. The inner loops read integers from
  flat arrays and never walk the tree.
- **One core for left and right paths.** `spf_l` and `spf_r` share a single
  implementation over a flat forest-distance table, with loop-invariant data
  hoisted out of the loops (-13.2% total).
- **Contiguous strategy matrix.** Strategy values and subtree distances live
  in one row-major matrix instead of nested vectors (-6.5%).
- **Label hashing.** `StringNodeData` stores a hash of its label. The
  innermost rename test compares hashes first and compares bytes only when
  they match, which turns most tests into an integer compare (-11.7%).
- **Row splitting in `spf_l`/`spf_r`.** The inner loop works on slices split
  at the current row, so LLVM can drop the bounds checks (-2.4%). The same
  transformation made the `spf_a` loop slower, so `spf_a` keeps its original
  form.
- **Iterative traversal and checked allocation.** Every tree walk uses an
  explicit stack and allocation sizes are checked up front. Together they
  cost about 2% in total (an interleaved A/B measured +1.8%, within noise;
  per pair -10% to +5%) and make arbitrary depth and untrusted sizes safe.

`zigzag-zigzag` and `left-right` spend most of their time in `spf_a`, a
dependent chain of min operations over forest-distance rows. Java's JIT
generates comparable code for that loop, so the two implementations are even
there; faster results need a different loop structure.

## Noise

Per-cell difference between the two runs, relative to their mean: median
3%, maximum 21% (Java `right-right`, a 34-43 ms pair). Totals differ by at
most 3.3% for Java and at most 1% for every Rust column except
`+ unified spf_l/spf_r` (2.8%). A total-time difference below about 3%,
such as `+ strategy matrix` against Java, is within noise on this machine;
every other conclusion above holds in both runs.

## Caveats

- One VM on a shared cloud host; neighbours can steal cycles. Interleaving
  and best-of-medians reduce this effect without removing it.
- The JVM uses background threads for JIT compilation and GC on the other
  vCPUs; Rust runs on one thread. Java's warm-up is excluded; Rust has no
  warm-up and pays its first-run page faults inside the timing.
- Only the unit cost model and 1,000-node trees were measured. Larger trees
  shift time further toward the inner-path shapes.
- Ratios move between CPU microarchitectures. Treat them with the
  `env.txt` they came with.
- The `literal port` and intermediate columns were measured from builds
  that are not part of this repository, so only `apted` against Java
  (and against any revision, see [benchmarking.md](benchmarking.md)) can be
  rerun.

## Reproduce

```sh
WORK=$HOME/apted-bench SAVE=1 compare-java/bench.sh 5 1000 3   # apted against Java
```

See [benchmarking.md](benchmarking.md) for requirements and options.
