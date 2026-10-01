# Contributing

`apted` is a dependency-free Rust port of the Java APTED implementation.
Bug reports, tests and focused patches are welcome.

## Checks

Run all of these before sending a change:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test             # debug: overflow checks on
cargo test --release   # release: the configuration users run
```

Index arithmetic mixes `i32` and `usize`. Release builds wrap on overflow
silently, so a bug can pass in release and only panic in debug; run both.

The crate has no dependencies, including dev-dependencies, so everything
builds offline. Tests read their JSON fixtures with the small reader in
`tests/common/json.rs`.

## Layout

- `src/distance/apted.rs`: the algorithm (strategy computation, `spf1`,
  `spf_l`/`spf_r`, `spf_a`, `gted`, edit mapping).
- `src/node/`: `Node<D>`, `StringNodeData`, and the internal `NodeIndexer`
  that precomputes the index arrays the algorithm reads.
- `src/cost_model.rs`, `src/parser.rs`, `src/error.rs`: cost models, the
  bracket-notation parser, the fallible-API errors and memory estimate.
- `tests/`: ported Java suite, randomized differential tests, Java golden
  equivalence, regressions and robustness tests.
- `fuzz/`: cargo-fuzz targets. `bench/`, `examples/bench.rs`, `docs/`:
  benchmarks and their documentation. `compare-java/`: optional tooling
  against the upstream Java (needs a JDK or Docker).

## Rules

- Keep names and structure close to the Java original; comments cite the
  Java method each function ports, so the two can be compared side by side.
- Results must stay bit-identical to Java except for the documented
  differences in the README. `tests/java_golden.rs` enforces this against
  recorded Java output, and its exceptions are explicit.
- Never recurse over tree depth. Every tree walk uses an explicit stack, so
  deep input cannot overflow the stack (`tests/robustness.rs` runs on a
  256 KiB thread). Allocation sizes derived from tree sizes go through the
  checks in `src/error.rs`, never a raw `vec!` on untrusted products.
- The distance for every benchmark pair must not change. `examples/bench.rs`
  prints `d=`, and `bench/run_all.sh` fails on any `DISTANCE MISMATCH`.
- Judge performance with `bench/ab.sh` (interleaved A/B), never by comparing
  two separate runs; single runs vary by up to about 15%. Describe a new
  technique and its measured effect in `docs/performance-report.md`
  ("Why it is fast").
- The Rust blocks in the README are doc-tests. Edit an example and its prose
  together.
- Public items need documentation (`#![deny(missing_docs)]`). Items marked
  `#[doc(hidden)]` are test hooks and not part of the supported API.
- The minimum supported Rust version is declared as `rust-version` in
  `Cargo.toml` and checked in CI.

## Fuzzing

See [fuzz/README.md](fuzz/README.md). A crash becomes a regression test in
`tests/regressions.rs` before the fix.
