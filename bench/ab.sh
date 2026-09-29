#!/usr/bin/env bash
# A/B benchmark: committed HEAD (A) against the working tree (B).
# Runs the two binaries interleaved and reports, per pair, the best of the
# per-run medians. Usage: bench/ab.sh [rounds] [size] [reps] [filter]
set -euo pipefail
cd "$(dirname "$0")/.."
rounds=${1:-5}; size=${2:-1000}; reps=${3:-3}; filter=${4:-}
# Cargo and rustup read .cargo/config.toml and rust-toolchain.toml from
# every parent of the directory cargo runs in. Build both sides from the
# repository (with --manifest-path for A, whose worktree lives under the
# world-writable temp dir) and pin the toolchain, so files planted in the temp
# directory cannot inject a rustc wrapper, linker or toolchain.
RUSTUP_TOOLCHAIN=$(rustup show active-toolchain 2>/dev/null | cut -d' ' -f1 || true)
[ -n "$RUSTUP_TOOLCHAIN" ] && export RUSTUP_TOOLCHAIN || unset RUSTUP_TOOLCHAIN
work=$(mktemp -d)
trap 'git worktree remove --force "$work/a" >/dev/null 2>&1 || true; rm -rf "$work"' EXIT
git worktree add -q --detach "$work/a" HEAD
CARGO_TARGET_DIR="$work/target-a" cargo build -q --release --offline --example bench \
  --manifest-path "$work/a/Cargo.toml"
CARGO_TARGET_DIR="$work/target-b" cargo build -q --release --offline --example bench
a="$work/target-a/release/examples/bench"
b="$work/target-b/release/examples/bench"
for ((r = 0; r < rounds; r++)); do
  "$a" "$size" "$reps" "$filter" | sed 's/^/A /' >>"$work/out"
  "$b" "$size" "$reps" "$filter" | sed 's/^/B /' >>"$work/out"
done
awk '$2 != "ALL" {
       v = $6 + 0; k = $2
       key = $1 SUBSEP k
       if (!(key in best) || v < best[key]) best[key] = v
       names[k] = 1
     }
     END {
       printf "%-16s %10s %10s %8s\n", "pair", "A ms", "B ms", "B/A"
       for (k in names) {
         a = best["A" SUBSEP k]; b = best["B" SUBSEP k]; ta += a; tb += b
         printf "%-16s %10.2f %10.2f %8.3f\n", k, a, b, b / a
       }
       printf "%-16s %10.2f %10.2f %8.3f\n", "TOTAL", ta, tb, tb / ta
     }' "$work/out"
