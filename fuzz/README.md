# Fuzzing

cargo-fuzz (libFuzzer) targets for `apted`. This crate is separate from the
library: `apted` stays dependency-free, and only `fuzz/` depends on
`libfuzzer-sys`.

## Setup

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
```

## Targets

| Target | Input | Checks |
|---|---|---|
| `parser` | arbitrary bytes | `try_from_string` never panics; every accepted tree prints and parses back equal; trees with arbitrary-text labels round-trip through `Display` |
| `distance` | bytes decoded into three small trees and a cost model | `try_` API with a memory limit never panics; d(t,t) = 0, symmetry, size bounds, triangle inequality, refused by a 1-byte limit; the mapping uses every node once, keeps ancestor and sibling order (`check_mapping` in `tests/oracle/`), and costs the distance |
| `differential` | bytes decoded into trees and a cost model | APTED, spfL and spfR equal the Zhang-Shasha oracle (`tests/oracle/`); APTED equals the brute-force `AllPossibleMappingsTED` on trees of up to 5 nodes |

The cost model is symmetric with rename cost at most delete plus insert, so
the metric properties hold.

## Running

```sh
cd fuzz
cargo +nightly fuzz run parser -- -max_len=4096 -dict=dict/parser.dict
cargo +nightly fuzz run distance -- -max_len=1024
cargo +nightly fuzz run differential -- -max_len=1024
```

Add `-max_total_time=600` to stop after ten minutes. Builds use release
optimization with debug assertions and overflow checks on, which is the
configuration that catches index arithmetic bugs.

A crash is saved under `fuzz/artifacts/<target>/`. Minimize it with
`cargo +nightly fuzz tmin <target> <file>`, turn it into a regression test in
`tests/regressions.rs`, then fix the code.

`fuzz/corpus/` is not committed (it is regenerated in seconds); the parser
dictionary in `dict/` is.
