# Changelog

This project follows [Semantic Versioning](https://semver.org).

## [0.1.1]

First public release.

### Added

- `APTED`: exact tree edit distance (Pawlik and Augsten) and edit mapping
  for ordered, labeled trees, with the algorithm's per-subtree strategy
  selection.
- Cost models over any node data type: `CostModel`, `StringUnitCostModel`,
  `PerEditOperationStringNodeDataCostModel`.
- Bracket-notation parser with `\{`, `\}` and `\\` escapes.
- Fallible API for untrusted input: `try_from_string`,
  `try_compute_edit_distance`, `try_compute_edit_mapping`,
  `with_memory_limit`, `estimated_peak_bytes`, `TedError`, `ParseError`.
  Every tree walk is iterative, so tree depth is unlimited.
- Results equal to the Java reference implementation on recorded output, with
  one documented exception: the upstream `spf1` defect, which undercounts the
  distance for some non-unit cost models, is fixed (see the README,
  "Differences from the Java implementation").
- 1.13-1.16x lower total time than the Java implementation on the benchmark
  suite (see the README, "Performance").
- No dependencies, no `unsafe` code. Minimum supported Rust version 1.70.
