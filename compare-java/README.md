# compare-java

Optional tooling that runs the upstream Java APTED next to this crate. The
crate, its tests and its Rust-only benchmarks (`bench/`) never need any of
it, and `cargo package` leaves it out.

- Upstream: <https://github.com/DatabaseGroup/apted>, commit
  `193666ba2b68aa8106dc2d1d0a78b8a0e07c286b`, MIT.
- Needs: a JDK 11+, or Docker.

## Speed comparison

```sh
compare-java/bench.sh 5 1000 3        # host JDK
JAVA_DOCKER=default compare-java/bench.sh 5 1000 3   # JDK in docker (a docker context)
```

Runs `bench/run_all.sh` with Java as the first column and this checkout
(plus any `REVS`) after it. Arguments and output are those of `run_all.sh`; see
[docs/benchmarking.md](../docs/benchmarking.md). `SAVE=1` writes the result
to `bench/results/`.

With `JAVA_DOCKER=<context>`, sources, the harness and the trees stream into
a throwaway container (`docker --context <context> run --rm -i
eclipse-temurin:21-jdk`); nothing is mounted and nothing runs on the host
JVM. `JAVA_IMAGE` overrides the image.

| File | Purpose |
|---|---|
| `bench.sh` | entry point |
| `bench-java.sh` | hook `run_all.sh` calls: clone upstream at the pinned commit, compile, run, report the environment |
| `AptedBench.java` | Java harness; prints the same lines as `examples/bench.rs` |

## Saved measurement

`bench/results/x86-64-vm-run1` and `-run2` hold the two runs behind the
README's speed figures: a 4-vCPU cloud VM, upstream `193666b`, OpenJDK 21.
Their columns are described in
[docs/benchmarking.md](../docs/benchmarking.md#columns).

## Differential run and golden files

`rust-driver/` and `GoldenDump.java` print, per case, float bits of the
distance, the reversed distance, the forced left-path and right-path
distances, the mapping cost, the exact mapping pair list and, for trees of
at most 6 nodes, the brute-force distance. Same input, same line format, so
two outputs compare with `compare.py`.

```sh
W=$(mktemp -d)

# Inputs (deterministic), about 45,000 cases; shapes from the crate's bench.
cargo run --release --example bench -- --dump "$W/shapes" 1000
python3 compare-java/gen_cases.py . "$W/shapes" "$W/input.tsv"

# Java, in docker (the JDK never runs on the host)
tar -c -C compare-java/ref-java src/main/java/at -C compare-java GoldenDump.java -C "$W" input.tsv |
  docker --context default run --rm -i eclipse-temurin:21-jdk sh -c '
    mkdir /w && cd /w && tar xf - && mkdir cls &&
    javac -nowarn -d cls $(find src -name "*.java") GoldenDump.java &&
    java -Xss512m -cp cls GoldenDump <input.tsv' >"$W/java.out"

# Rust, this checkout (or edit the path in rust-driver/Cargo.toml)
(cd compare-java/rust-driver && cargo run --release -q) <"$W/input.tsv" >"$W/rust.out"
python3 compare-java/compare.py "$W/java.out" "$W/rust.out" --floats
```

`compare-java/ref-java` is a checkout of upstream at the pinned commit
(`regen-goldens.sh` creates it; it is gitignored). The full run takes about 17
minutes for Java on 4 vCPUs.

| File | Purpose |
|---|---|
| `GoldenDump.java` | Java driver |
| `rust-driver/` | the same driver for this crate |
| `gen_cases.py` | input generator; `--golden` selects the committed subset |
| `compare.py` | field-by-field comparison of two outputs |
| `regen-goldens.sh` | rewrites the Java columns of `tests/resources/java_golden/cases.tsv` |

The committed golden subset, the result of the full run and the documented
differences are in
[tests/resources/java_golden/README.md](../tests/resources/java_golden/README.md).
`tests/java_golden.rs` keeps the crate pinned to it without a JDK.
