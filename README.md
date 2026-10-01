# apted

[![crates.io](https://img.shields.io/crates/v/apted.svg)](https://crates.io/crates/apted)
[![docs.rs](https://docs.rs/apted/badge.svg)](https://docs.rs/apted)
[![CI](https://github.com/mike-weinberg/apted-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/mike-weinberg/apted-rs/actions/workflows/ci.yml)
[![license](https://img.shields.io/crates/l/apted.svg)](https://github.com/mike-weinberg/apted-rs/blob/main/LICENSE)
[![MSRV](https://img.shields.io/badge/rustc-1.70+-blue.svg)](https://github.com/mike-weinberg/apted-rs/blob/main/Cargo.toml)

Exact tree edit distance in Rust: **APTED** (All Path Tree Edit Distance) by
Mateusz Pawlik and Nikolaus Augsten, a port of the reference Java
implementation ([DatabaseGroup/apted](https://github.com/DatabaseGroup/apted)).
No dependencies, no `unsafe` code.

The tree edit distance between two ordered, labeled trees is the minimum
total cost of node deletions, insertions and renames that turns one into the
other. APTED computes it exactly and picks a decomposition strategy per
subtree pair, which keeps the work low on any tree shape.

- Distance and **edit mapping** (which node became which).
- Pluggable cost models over any node data type.
- Bracket-notation parser, with a fallible API for untrusted input.
- Matches the Java reference bit for bit, apart from the documented
  [differences](#differences-from-the-java-implementation) (see
  [Correctness](#correctness)).
- 1.13-1.16x faster than the Java implementation on the benchmark suite (see
  [Performance](#performance)).

## Install

```sh
cargo add apted
```

or in `Cargo.toml`:

```toml
[dependencies]
apted = "0.1"
```

Requires Rust 1.70 or newer.

## Usage

### Distance between bracket-notation trees

```rust
use apted::{BracketStringInputParser, StringUnitCostModel, APTED};

let p = BracketStringInputParser::new();
let t1 = p.from_string("{a{b}{c}}");
let t2 = p.from_string("{a{b{d}}}");

let d = APTED::new(StringUnitCostModel).compute_edit_distance(&t1, &t2);
assert_eq!(d, 2.0); // delete c, insert d under b
```

Bracket notation: `{label child child ...}` where each child is itself
bracketed, e.g. `{html{head}{body{p}{p}}}`. Write `\{`, `\}` and `\\` for
literal braces and backslashes in labels; `to_string()` escapes them, so
printed trees always parse back (Java has no escapes, see
[Differences](#differences-from-the-java-implementation)). `from_string`
panics on malformed input; use `try_from_string` for untrusted text.

### Building trees in code

```rust
use apted::{Node, StringNodeData, StringUnitCostModel, APTED};

fn node(label: &str, children: Vec<Node<StringNodeData>>) -> Node<StringNodeData> {
    let mut n = Node::new(StringNodeData::new(label));
    for c in children {
        n.add_child(c);
    }
    n
}

let t1 = node("add", vec![node("x", vec![]), node("1", vec![])]);
let t2 = node("add", vec![node("y", vec![]), node("1", vec![])]);
assert_eq!(APTED::new(StringUnitCostModel).compute_edit_distance(&t1, &t2), 1.0);
```

### Edit mapping

```rust
use apted::{BracketStringInputParser, StringUnitCostModel, APTED};

let p = BracketStringInputParser::new();
let (t1, t2) = (p.from_string("{a{b}{c}}"), p.from_string("{a{c}{d}}"));
let mut apted = APTED::new(StringUnitCostModel);
let d = apted.compute_edit_distance(&t1, &t2); // must come first
let mapping = apted.compute_edit_mapping();
// Pairs of 1-based postorder ids; 0 = no partner (deleted or inserted).
assert_eq!(apted.mapping_cost(&mapping), d);
```

Mapping pairs are 1-based postorder ids with `0` for "no partner"
(`[x, 0]` deletes `x`, `[0, y]` inserts `y`, `[x, y]` keeps or renames).
Distances are directed only through the cost model: with equal delete and
insert costs, `d(t1, t2) == d(t2, t1)`.

### Cost models

| Model | Costs |
|---|---|
| `StringUnitCostModel` | delete 1, insert 1, rename 1 (0 for equal labels) |
| `PerEditOperationStringNodeDataCostModel::new(del, ins, ren)` | fixed per-operation costs |
| your own `impl CostModel<D>` | anything, over any node data `D` |

```rust
use apted::{CostModel, Node};

struct Weighted;
impl CostModel<String> for Weighted {
    fn del(&self, _: &Node<String>) -> f32 { 1.0 }
    fn ins(&self, _: &Node<String>) -> f32 { 1.0 }
    fn ren(&self, a: &Node<String>, b: &Node<String>) -> f32 {
        if a.node_data() == b.node_data() { 0.0 } else { 0.5 }
    }
}
```

`ren` runs in the innermost loop; keep it cheap.

## Limits and untrusted input

- Distances are `f32`; costs must not be NaN.
- Worst-case time O(n³). Memory about 12·n·m bytes, plus up to
  4·(max(n, m) + 1)² for inner paths: two 10,000-node trees need 1.2 GB
  or more. `compute_edit_mapping` adds 4·(n+1)·(m+1) bytes.
- Tree depth is unlimited: every tree walk is iterative.
- For untrusted input, enforce a node budget, then use the fallible API:

```rust
use apted::{BracketStringInputParser, StringUnitCostModel, APTED};

let p = BracketStringInputParser::new();
let t1 = p.try_from_string("{a{b}}").expect("valid input");
let t2 = p.try_from_string("{a{c}}").expect("valid input");
let mut apted = APTED::new(StringUnitCostModel).with_memory_limit(256 << 20);
match apted.try_compute_edit_distance(&t1, &t2) {
    Ok(d) => assert_eq!(d, 1.0),
    Err(e) => eprintln!("rejected: {e}"), // too large, over the limit, ...
}
```

See [SECURITY.md](https://github.com/mike-weinberg/apted-rs/blob/main/SECURITY.md) for the threat model.

## Differences from the Java implementation

Results are bit-identical to Java's except in the cases below. Recorded
Java output for 3,635 cases, and how it was produced, is in
[`tests/resources/java_golden/`](https://github.com/mike-weinberg/apted-rs/blob/main/tests/resources/java_golden/README.md);
`tests/java_golden.rs` checks every distance and mapping against it.

- **Upstream `spf1` bug, fixed.** Upstream `spf1` (the single-node case of a subtree pair)
  undercounts when three things hold: no node of the other subtree has an
  equal label, every `ren(n1, n2) - ins(n2)` exceeds the summed insertion
  cost of that subtree, and that sum is below `del(n1)`. The mirror case
  applies to deletions. A rename cheaper than delete plus insert can
  qualify. `spf1` runs on every subtree pair the decomposition reaches where
  either side is a single node, so input trees of any size can trigger it,
  and the unit cost model never does. The port fixes it
  (`tests/regressions.rs`).

  ```rust
  use apted::{BracketStringInputParser, PerEditOperationStringNodeDataCostModel, APTED};

  let p = BracketStringInputParser::new();
  let (t1, t2) = (p.from_string("{b}"), p.from_string("{c{a}}"));
  let cost = PerEditOperationStringNodeDataCostModel::new(1.0, 0.1, 0.5);
  let d = APTED::new(cost).compute_edit_distance(&t1, &t2);
  assert!((d - 0.6).abs() < 1e-6); // upstream returns 0.4
  ```

  In 45,112 differential cases, 90 differed from Java for this reason;
  Java's distance was lower each time. Mappings never differed.
- **Backslashes in labels.** A backslash before `{`, `}` or `\` is an
  escape here, so `\\` is one backslash. Java has no escapes and keeps every
  backslash. A label containing a backslash parses differently, and
  `to_string` writes it escaped.
- **Trees above about 46,000 nodes.** Strategy cost sums are `i64`. Java's
  `int` sums overflow in subtrees of that size, so Java picks another
  decomposition strategy there. The distance stays exact, but `f32` rounding
  and the choice between equal-cost mappings can differ from Java. No
  recorded case covers trees this large.
- **Mapping before distance.** `compute_edit_mapping` panics with a message
  if no distance was computed for the current trees; `try_compute_edit_mapping`
  returns an error. Java's behaviour there is undefined.
- The brute-force test oracle `AllPossibleMappingsTED` starts its minimum at
  infinity. Java's starts at `size1 + size2`, which is wrong for operation
  costs above 1.
- The parser is strict (Java silently misparses malformed input); tree
  walks are iterative (Java recurses); oversized input yields an error from
  the `try_` API instead of an exception.
- Rust API names are snake_case (`compute_edit_distance`); the structure,
  and comments naming the Java method each function ports, follow the
  original closely.

## Correctness

Correctness is checked against four independent references:

- **Java test suite.** `tests/correctness.rs` and
  `tests/per_edit_operation_correctness.rs` port the upstream tests and run
  them on the upstream JSON fixtures: 77 cases (distance, forced left and right
  paths, mapping cost), and 75 against the brute-force
  `AllPossibleMappingsTED`.
- **Java reference output.** Distances and edit mappings equal the Java
  implementation's on 45,112 cases (8 cost models, trees of 1 to 1,000
  nodes), bit for bit, except the 90 cases of the
  [`spf1` defect](#differences-from-the-java-implementation), where only the
  distance differs. A 3,635-case subset of Java's output is checked in and
  verified by `tests/java_golden.rs`; see the
  [golden files](https://github.com/mike-weinberg/apted-rs/blob/main/tests/resources/java_golden/README.md).
- **Independent algorithms.** `tests/randomized.rs` compares APTED, and APTED
  forced onto left or right paths, with an independent Zhang-Shasha
  implementation on 820 random tree pairs under unit and per-operation cost
  models, and with the brute-force oracle on 150 tiny pairs. 1,220 computed
  edit mappings are checked for validity (one-to-one, ancestor and sibling
  order kept) and for costing the distance.
- **Fuzzing.** Three cargo-fuzz targets (parser, distance, differential); see
  [fuzz/README.md](https://github.com/mike-weinberg/apted-rs/blob/main/fuzz/README.md) and
  [SECURITY.md](https://github.com/mike-weinberg/apted-rs/blob/main/SECURITY.md).

`tests/robustness.rs` covers 200,000-level trees on a 256 KiB thread,
oversized and hostile input, NaN costs and stale mappings. Run the suite in
debug and release, because release builds wrap integer overflow silently:

```sh
cargo test && cargo test --release
```

## Performance

On nine pairs of 1,000-node trees (random, left, right, binary, zigzag and
flat shapes) with the unit cost model, `apted` takes 7.4-7.5 s in total
against 8.4-8.7 s for the Java implementation: **1.13-1.16x faster** over two
runs. It is 1.2-2.1x faster on the cheap shapes and on par on the inner-path
shapes (`zigzag-zigzag`, `left-right`), which dominate the total. It is 1.43x
faster than a literal port of the Java code, a line-by-line translation kept
for comparison.

Environment: 4-vCPU x86-64 cloud VM (Intel Xeon at 2.10 GHz, Linux),
rustc 1.94.1 with the default release profile, OpenJDK 21 (upstream Java
APTED commit `193666b`). Each implementation reports the median of 3 runs per
pair, the set runs 5 times interleaved and the best median counts; Java gets
2 untimed warm-up runs per pair. Ratios move between CPU microarchitectures.

Per-pair results and the techniques behind them are in the
[performance report](https://github.com/mike-weinberg/apted-rs/blob/main/docs/performance-report.md). To reproduce the
measurement, see [docs/benchmarking.md](https://github.com/mike-weinberg/apted-rs/blob/main/docs/benchmarking.md):

```sh
compare-java/bench.sh 5 1000 3                  # this checkout against Java (needs a JDK or Docker)
cargo run --release --example bench -- 1000 5   # quick timing of this checkout
bench/ab.sh 5 1000 3                            # HEAD against the working tree
```

## Security

No `unsafe` code (`#![forbid(unsafe_code)]`), no dependencies, no I/O,
iterative traversal for any tree depth, size and memory checks in the
`try_` API. See [SECURITY.md](https://github.com/mike-weinberg/apted-rs/blob/main/SECURITY.md) for the threat model, the
fuzzing results and how to report a vulnerability.

## Contributing

See [CONTRIBUTING.md](https://github.com/mike-weinberg/apted-rs/blob/main/CONTRIBUTING.md). Changes:
[CHANGELOG.md](https://github.com/mike-weinberg/apted-rs/blob/main/CHANGELOG.md).

## Credits

- M. Pawlik and N. Augsten. *Efficient Computation of the Tree Edit
  Distance.* ACM TODS 40(1), 2015.
- M. Pawlik and N. Augsten. *Tree edit distance: Robust and
  memory-efficient.* Information Systems 56, 2016.

## License

MIT, Copyright (c) 2026 Michael Adin Weinberg; see [LICENSE](https://github.com/mike-weinberg/apted-rs/blob/main/LICENSE).

This is a port of the Java APTED implementation, Copyright (c) 2017 Mateusz
Pawlik and Nikolaus Augsten, also MIT; see
[LICENSE-APTED-JAVA](https://github.com/mike-weinberg/apted-rs/blob/main/LICENSE-APTED-JAVA). The JSON fixtures in
`tests/resources/` come from the upstream test suite under that notice.
