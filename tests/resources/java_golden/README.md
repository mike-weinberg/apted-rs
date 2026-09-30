# Java golden files

Recorded output of the upstream Java APTED, for `tests/java_golden.rs`. The
test runs this crate on the same inputs and requires every field to match
bit for bit, apart from the documented differences below.

## Files

| File | Content |
|---|---|
| `cases.tsv` | one case per line: `kind t1 t2 model d drev spfL spfR mc map apm` |
| `spf1_divergences.tsv` | the cells where this crate differs on purpose |

Columns of `cases.tsv`:

- `model` is `unit` or `del,ins,ren` (`PerEditOperationStringNodeDataCostModel`).
- `d`, `drev`, `spfL`, `spfR`, `mc`, `apm` are `Float.floatToIntBits` in hex:
  distance, reversed distance, distance with every strategy path forced left
  or right, cost of the edit mapping, brute-force distance.
- `map` is the exact mapping pair list, `row:col,...`, in Java's order.
- `-` marks a field Java did not compute: `spfL` and `spfR` above 300 nodes,
  `apm` above 6 nodes per tree.

3,635 cases: the upstream JSON fixtures under five cost models, every
single-node-side case against all trees of up to 5 nodes, random pairs of 1
to 40 nodes, 100-node benchmark shapes and random pairs of 100 to 300
nodes. Labels are `[a-z0-9]`.

## Documented differences

- `spf1`: upstream undercounts the distance when no label-equal node pair
  exists, every `ren(n1, n2) - ins(n2)` exceeds the summed insert cost of the
  subtree, and that sum is below `del(n1)` (mirrored for deletes). The
  cells are listed in `spf1_divergences.tsv`. The test requires Rust to be
  larger and equal to an independent Zhang-Shasha distance there, and Java
  to be wrong. Mapping fields never differ.
- `apm`: Java's brute force starts its minimum at `size1 + size2`; this crate
  starts at infinity. The test requires `java == min(rust, size1 + size2)`.

## Provenance

| | |
|---|---|
| Upstream | `DatabaseGroup/apted` at `193666ba2b68aa8106dc2d1d0a78b8a0e07c286b` |
| JDK | `eclipse-temurin:21-jdk` (OpenJDK 21.0.12) in Docker |
| Inputs | `compare-java/gen_cases.py REPO SHAPES --golden`, shapes from `cargo run --release --example bench -- --dump SHAPES 100` |
| Outputs | `JAVA_DOCKER=<context> compare-java/regen-goldens.sh` |

`regen-goldens.sh` reruns Java on the inputs recorded in `cases.tsv`;
`--check` compares without writing. Details: [compare-java/README.md](../../../compare-java/README.md).

## Full differential run

The golden file is a subset. The same harness ran 45,112
cases (8 cost models: unit and seven non-dyadic triples including
`0.1,0.2,0.3` and `1.3,0.17,0.91`; 5,000 random pairs of 1 to 40 nodes; 200
pairs of 100 to 300 nodes; the nine 1,000-node benchmark shapes; both
fixture files; single-node-side cases) on three builds: Java `193666b`, this
crate with the `spf1` fix removed ("unfixed") and this crate ("fixed").

| Comparison | Result |
|---|---|
| Java vs unfixed, `d` `drev` `spfL` `spfR` `mc` `map` | bit-identical on all 45,112 cases (`spfL`/`spfR`: 45,040 computed, 72 skipped) |
| Java vs unfixed, `apm` (trees of at most 6 nodes) | bit-identical, 5,320 of 5,320 |
| Java vs fixed, mapping pair lists | identical on all 45,112 cases, so no tie-breaking difference, including the 90 cases below |
| Java vs fixed, `d` `drev` `spfL` `spfR` | differ in 41, 49, 41, 41 cases; 90 distinct cases (fixtures 16, single-node 68, random small 6, large 0, shapes 0); none at unit cost; the fixed crate is larger in every one, by at most 1.0 |
| Java vs fixed, `apm` | differs in 417 of 5,320; all equal `min(fixed, size1 + size2)` |

Every difference is an `spf1` case. Zhang-Shasha (independent, f64) agrees
with the fixed crate on every case (shapes excluded) and with Java on every case
except the differing ones. Per run, the differing set equals the set where the
`spf1` fix changed some single-node subtree call and Java disagrees with
Zhang-Shasha; that trigger fired in 7,856 cases but was masked by the final
minimum in 7,766 of them. No difference lies outside the trigger condition
above (20,491 cases satisfy it structurally on some subtree pair, a superset).
The instrumented build and the Zhang-Shasha run were scratch tools and are
not committed.

The mapping cost equals the distance at the fixed crate in every case but one
(`large`, 7.0999064 against 7.1000004: `f32` accumulation over 300 nodes,
identical in Java). In Java the distance is below the mapping cost in the 41
cases where `d` differs, because the mapping itself is right.
