# Benchmarking

The benchmark suite times this crate on identical input and, optionally,
compares it with other revisions of itself and with the upstream Java
implementation. Inputs are generated from a fixed seed and each run records
its environment next to its numbers. It needs no JDK and no network.

The upstream Java APTED joins as an extra first column through optional
tooling in [`compare-java/`](../compare-java/README.md). Results are in
[performance-report.md](performance-report.md).

## Reproduction

```sh
curl https://sh.rustup.rs -sSf | sh    # if cargo is missing

git clone https://github.com/mike-weinberg/apted-rs && cd apted-rs
cargo test && cargo test --release
WORK=$HOME/apted-bench SAVE=1 bench/run_all.sh 5 1000 3          # this checkout
REVS="<rev>:<label>" bench/run_all.sh 5 1000 3                   # plus a revision, e.g. REVS="main~1:base"
compare-java/bench.sh 5 1000 3                                   # plus Java
```

The report is printed at the end and saved to `bench/results/<host>-run<N>/`
with `SAVE=1`. Speedups are relative to the first column. For a Java column,
run `compare-java/bench.sh` with the same arguments (see
[`compare-java/README.md`](../compare-java/README.md)).

The crate has no dependencies, so nothing is fetched from crates.io and all
Rust builds run with `--offline`.

## What is measured

**Input.** `examples/bench.rs` generates nine tree pairs from six shapes,
each tree with `size` nodes (default 1000) and labels drawn from 20
single-character strings. The seed depends only on `size`.

| Shape | Structure | Stresses |
|---|---|---|
| `random` | each node's parent chosen uniformly among earlier nodes | typical mixed case |
| `left` | spine through each node's first child, a leaf beside it | left paths (`spf_l`) |
| `right` | spine through each node's last child | right paths (`spf_r`) |
| `binary` | complete binary tree | balanced, shallow |
| `zigzag` | spine alternates between first and last child | inner paths (`spf_a`), APTED's worst case |
| `flat` | root with n-1 leaves | wide, depth 1 |

Pairs: the six same-shape pairs plus `left-right`, `zigzag-binary` and
`random-zigzag`.

**Operation.** One `compute_edit_distance` call with the unit cost model,
constructing a fresh `APTED` each time (Java: `new APTED<>(new
StringUnitCostModel())`). Tree construction and parsing are not timed.

**Statistic.** Each implementation runs every pair `reps` times and reports
the median. The whole set of implementations runs `rounds` times,
interleaved (each implementation in turn, then again), and the report
keeps the **best median** per pair and implementation. Interleaving spreads
slow drift (thermal, noisy neighbours) evenly over all implementations;
taking the best median discards rounds disturbed by transient load.

**JIT (with `compare-java/`).** Java gets 2 untimed warm-up computations per
pair before the timed ones, so its numbers reflect JIT-compiled code. The
JVM runs with `-Xss64m -Xmx8g` and its default collector; JIT and GC threads
may use other cores. Rust is compiled ahead of time with the default release
profile and runs on one thread.

**Correctness.** Every implementation prints the distance it computed. The
report fails with `DISTANCE MISMATCH` if any implementation disagrees on any
pair.

## Columns

| Label | Implementation |
|---|---|
| `java` | upstream `193666b`, the reference (only with `compare-java/`) |
| `head` | this checkout |
| one per `REVS` entry | that revision, built from a detached `git worktree` |

`bench/run_all.sh` builds each requested revision against the current
`examples/bench.rs` (through a throwaway harness crate), so all columns use
the same harness and differ only in the library. A revision must offer the
API that file uses.

The saved runs in `bench/results/` also carry a `literal-port` column, a
line-by-line translation of the Java code, and four technique columns
that add one technique each, cumulatively: `+spf_lr` (one core for left and right paths),
`+matrix` (one contiguous strategy matrix), `+labelhash` (labels compared by
precomputed hash before bytes) and `+splitrows` (split-row slices in the
`spf_l`/`spf_r` inner loop, the implementation in this crate).
[performance-report.md](performance-report.md#why-it-is-fast) describes each.

## Files

| File | Purpose |
|---|---|
| `bench/run_all.sh` | this checkout plus optional revisions (`REVS`), Rust only |
| `bench/ab.sh` | interleaved A/B of `HEAD` vs the working tree |
| `bench/env.sh` | environment capture (CPU, OS, load, toolchains, commits) |
| `compare-java/` | optional Java comparison: `bench.sh`, `AptedBench.java`, see its README |
| `examples/bench.rs` | tree generator, `--dump` for other implementations, Rust timing |
| `bench/results/` | saved runs (`SAVE=1`), each with `env.txt`, `raw.txt`, `report.txt` |
| `docs/performance-report.md` | results and analysis |

## Getting stable numbers

- Close other workloads; check `uptime`. The suite records load at start.
- Prefer bare metal or a dedicated VM. On shared cloud hosts, repeat the
  run and compare the reports; only trust differences larger than the
  spread between runs.
- Do not change CPU frequency settings mid-run; on laptops stay on AC power.
- Relative results (speedup columns) transfer between machines far better
  than absolute milliseconds, but even ratios move between CPU
  microarchitectures. Always report them with the `env.txt` they came with.
