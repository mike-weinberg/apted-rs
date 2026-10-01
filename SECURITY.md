# Security

## Threat model

`apted` is a pure computation library with no I/O, no dependencies and no
`unsafe` code, so memory corruption is ruled out by the compiler. The crate
root carries `#![forbid(unsafe_code)]`, so adding `unsafe` fails the build. The
relevant risks for an application that feeds it **untrusted trees** (user
uploads, parsed documents, network input) are denial of service and
incorrect results:

| Risk | Status |
|---|---|
| Stack overflow on deep trees (uncatchable abort) | **Fixed.** Parsing, indexing, the GTED decomposition, keyroot computation and `Node`'s `Drop`/`Clone`/`PartialEq`/`Debug`/`Display`/`node_count` use explicit stacks. Tested with 200,000 levels on a 256 KiB thread. |
| Quadratic parsing of deeply nested input | **Fixed.** Single linear pass. |
| Process abort on huge allocations | **Fixed** in the `try_` API: sizes are checked with overflow-safe arithmetic and the allocator is asked up front; failures return `TedError`. The panicking API panics (catchable) instead of aborting. |
| `f32` node ids losing precision above 2²⁴ nodes | **Fixed:** rejected with `TedError::TooLarge`. |
| Integer overflow in strategy cost sums (wide trees) | **Fixed:** summed in `i64`. |
| NaN costs panicking in `compute_edit_mapping` | **Fixed:** `TedError::NotANumber`; the mapping walk stays in range. |
| Mapping requested for a stale or missing distance | **Fixed:** `TedError::DistanceNotComputed`. |
| Parser silently dropping or merging nodes; label injection through `Display` | **Fixed:** strict parser; `\{`, `\}`, `\\` escapes, emitted by `Display`. |
| CPU time: worst case O(n³) | **Inherent.** Callers must bound input size. |

## Using the library on untrusted input

1. Parse with `BracketStringInputParser::try_from_string`, or build `Node`s
   directly.
2. Enforce a **node budget** before computing (e.g. `t.node_count()`), sized
   to your latency limits: two 1,000-node trees take from milliseconds to
   ~2 s depending on shape (see `docs/performance-report.md`), and time can
   grow cubically.
3. Call `APTED::new(cm).with_memory_limit(bytes).try_compute_edit_distance(..)`
   and handle `TedError`. `apted::estimated_peak_bytes(n, m)` gives the
   baseline memory.
4. Keep custom cost models finite and non-negative; NaN is rejected, but
   negative or infinite costs give meaningless (though safe) results.

The allocation check asks the allocator for the memory up front. With
Linux memory overcommit that request can succeed although the pages are
not available later, and the kernel may still kill the process when they
are touched; `with_memory_limit` is the reliable bound.

## Scripts

`bench/*.sh` and `compare-java/*.sh` build and run code, so treat them like a
build:

- `bench/ab.sh` and `bench/run_all.sh` run cargo from the repository (never
  from the temp directory, whose parents are world-writable and could hold a
  planted `.cargo/config.toml` or `rust-toolchain.toml`) and pin the active
  toolchain.
- `run_all.sh` builds any extra revisions from detached worktrees it
  verifies before compiling. `compare-java/bench-java.sh` and
  `regen-goldens.sh` clone the upstream Java APTED the same way: pinned full
  commit hash, hooks and fsmonitor disabled, reused clone force-cleaned and
  verified.
- Paths written into generated `Cargo.toml` files are validated.

## Fuzzing

`fuzz/` holds three cargo-fuzz targets, described in
[fuzz/README.md](fuzz/README.md):

- `parser`: arbitrary bytes into `try_from_string`; never panics, accepted
  trees round-trip through `Display`.
- `distance`: small generated trees through the `try_` API with a memory
  limit; never panics, metric invariants hold, the mapping is valid
  (one-to-one, ancestor and sibling order kept) and costs the distance.
- `differential`: APTED against the Zhang-Shasha oracle and the brute-force
  algorithm.

To run a target (nightly toolchain and `cargo install cargo-fuzz`):

```sh
cd fuzz
cargo +nightly fuzz run parser -- -max_len=4096 -dict=dict/parser.dict
cargo +nightly fuzz run distance -- -max_len=1024
cargo +nightly fuzz run differential -- -max_len=1024
```

Builds run with debug assertions and overflow checks on. CI only checks that
the fuzz crate compiles; it does not fuzz.

## Reporting

Report vulnerabilities privately through GitHub: the repository's Security
tab, then "Report a vulnerability". If that option is missing, open an issue
that asks for a private contact and contains no details of the problem.

## Assurance

- **Code review.** All of `src/` and the benchmark scripts were reviewed for
  the risks in the threat model above. Each finding is fixed and has a
  regression test in `tests/robustness.rs` or `src/parser.rs`.
- **Fuzzing.** The three targets ran with libFuzzer on a nightly toolchain,
  with debug assertions and overflow checks on. Total executions without a
  crash or invariant failure in the library: `parser` 1,963,714, `distance`
  552,193, `differential` 1,141,241.
- **Oracle defect found by fuzzing.** The `differential` target found that the
  test oracle `AllPossibleMappingsTED` capped its result at `size1 + size2`,
  which is wrong for operation costs above 1. It is fixed, with a regression
  test in `tests/regressions.rs`.
