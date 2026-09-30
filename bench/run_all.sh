#!/usr/bin/env bash
# Benchmarks this checkout, and optionally other revisions of it, on identical
# trees: one column per implementation. Implementations run interleaved for
# ROUNDS rounds; the report shows, per pair, the best per-round median and
# the speedup against the first column.
#
# No JDK and no network are needed. An extra implementation (the upstream
# Java APTED, see compare-java/) joins as the first column through the
# EXTRA_IMPL hook below; compare-java/bench.sh sets it up.
#
# Usage: bench/run_all.sh [rounds] [size] [reps]
# Environment:
#   WORK=DIR     reuse DIR for clones, builds and results (default: mktemp)
#   REVS="REV:LABEL ..."  extra revisions of this repository to build and
#                compare, each from a detached worktree, e.g.
#                REVS="main~1:base main~5:older". Their examples/bench.rs
#                is not used: the harness is always built from this checkout,
#                so a revision must offer the API that file uses.
#   EXTRA_IMPL=SCRIPT  optional extra implementation, run as the first column:
#                SCRIPT setup WORK              prepare (clone, build)
#                SCRIPT run TREES_DIR REPS WORK print bench output lines
#                SCRIPT env WORK                print its environment
#                SCRIPT cleanup WORK            remove what setup created
#                (cleanup runs on exit when WORK is a throwaway dir)
#   EXTRA_LABEL=NAME   column name of the extra implementation (default extra)
#   SAVE=1       also copy env.txt, raw.txt and report.txt to
#                bench/results/<host>-run<N>/
# Needs: git, cargo, awk. Network: none (no crates.io access either).
set -euo pipefail
cd "$(dirname "$0")/.."
repo=$(pwd)
rounds=${1:-5}; size=${2:-1000}; reps=${3:-3}
EXTRA_IMPL=${EXTRA_IMPL:-}
EXTRA_LABEL=${EXTRA_LABEL:-extra}
# Revisions to compare with this checkout, as "rev:label" pairs.
read -r -a VERSIONS <<<"${REVS:-}"
# Pin the toolchain so a rust-toolchain.toml in a parent of WORK cannot
# select another one (cargo itself always runs from the repository).
RUSTUP_TOOLCHAIN=$(rustup show active-toolchain 2>/dev/null | cut -d' ' -f1 || true)
[ -n "$RUSTUP_TOOLCHAIN" ] && export RUSTUP_TOOLCHAIN || unset RUSTUP_TOOLCHAIN
# Git must not run hooks or an fsmonitor from a reused clone's config.
safe_git() { git -c core.hooksPath=/dev/null -c core.fsmonitor=false "$@"; }

work=${WORK:-}
if [ -z "$work" ]; then
  work=$(mktemp -d)
  # Throwaway dir: drop worktrees and builds on exit, keep the results.
  trap 'for w in "$work"/wt-*; do [ -d "$w" ] && git worktree remove --force "$w" >/dev/null 2>&1; done
        rm -rf "$work"/target-* "$work"/harness-*
        [ -n "$EXTRA_IMPL" ] && bash "$EXTRA_IMPL" cleanup "$work" >/dev/null 2>&1 || true' EXIT
fi
mkdir -p "$work"
work=$(cd "$work" && pwd)
# Both paths are written into a generated Cargo.toml as TOML strings.
case "$repo$work" in
  *'"'* | *\\* | *$'\n'*) echo "repository and WORK paths must not contain quotes, backslashes or newlines" >&2; exit 1 ;;
esac
out="$work/raw.txt"; : >"$out"
echo "work dir: $work" >&2

# Builds examples/bench.rs from this checkout against the library at $2 in
# a throwaway harness crate. A path dependency's dev-dependencies are not
# resolved, so this needs no crates.io access for any version.
build_harness() { # name lib-dir -> prints binary path
  local h="$work/harness-$1"
  mkdir -p "$h"
  cat >"$h/Cargo.toml" <<EOF
[package]
name = "apted-bench-harness"
version = "0.0.0"
edition = "2021"
publish = false

[[bin]]
name = "bench"
path = "$repo/examples/bench.rs"

[dependencies]
apted = { path = "$2" }

[workspace]
EOF
  CARGO_TARGET_DIR="$work/target-$1" cargo build -q --release --offline \
    --manifest-path "$h/Cargo.toml" >&2
  echo "$work/target-$1/release/bench"
}

# Identical input trees for every implementation.
dump=$(build_harness head "$repo")
"$dump" --dump "$work/trees" "$size"

# Extra implementation: let the hook prepare itself.
if [ -n "$EXTRA_IMPL" ]; then bash "$EXTRA_IMPL" setup "$work" >&2; fi

# Rust: this checkout first, then one binary per requested revision, each
# built from a detached worktree that is verified to be a clean checkout of
# exactly that commit.
bins=("head=$dump")
for v in ${VERSIONS[@]+"${VERSIONS[@]}"}; do
  label=${v##*:}; spec=${v%:*}
  rev=$(safe_git rev-parse --verify --quiet "$spec^{commit}") || {
    echo "unknown revision: $spec" >&2; exit 1; }
  wt="$work/wt-${rev:0:7}"
  if [ ! -d "$wt" ]; then
    # --force also replaces a registered worktree whose directory is gone.
    safe_git worktree add -q --force --detach "$wt" "$rev"
  fi
  if [ "$(safe_git -C "$wt" rev-parse HEAD)" != "$rev" ] ||
     [ -n "$(safe_git -C "$wt" status --porcelain)" ]; then
    echo "$wt is not a clean checkout of $rev; delete it and rerun" >&2; exit 1
  fi
  bins+=("$label=$(build_harness "${rev:0:7}" "$wt")")
done

bash bench/env.sh >"$work/env.txt"
if [ -n "$EXTRA_IMPL" ]; then bash "$EXTRA_IMPL" env "$work" >>"$work/env.txt"; fi
{
  echo
  echo "## Run parameters"
  echo "command: bench/run_all.sh $rounds $size $reps"
  echo "rust: release profile, no untimed warm-up (AOT compiled)"
  echo "revisions: head ${VERSIONS[*]:-}"
} >>"$work/env.txt"

# Prints "impl pair d=<distance> <median ms>" from benchmark output lines.
extract() {
  awk -v l="$1" '$1 != "ALL" && /median=/ {
    m = $0; sub(/.*median= */, "", m); split(m, a, " "); print l, $1, $3, a[1] }'
}
run_rust() { # label bin
  "$2" "$size" "$reps" | extract "$1"
}
run_extra() {
  bash "$EXTRA_IMPL" run "$work/trees" "$reps" "$work" | extract "$EXTRA_LABEL"
}
for ((r = 1; r <= rounds; r++)); do
  echo "round $r/$rounds" >&2
  if [ -n "$EXTRA_IMPL" ]; then run_extra >>"$out"; fi
  for b in "${bins[@]}"; do run_rust "${b%%=*}" "${b#*=}" >>"$out"; done
done

# Columns: impl pair d=<distance> median_ms. Distances must agree.
awk '
  { impl = $1; pair = $2; d = $3; ms = $4 + 0
    key = impl SUBSEP pair
    if (!(key in best) || ms < best[key]) best[key] = ms
    if (!(pair in dist)) dist[pair] = d; else if (dist[pair] != d) bad[pair] = bad[pair] " " impl ":" d
    if (!(impl in seen)) { seen[impl] = 1; order[++ni] = impl }
    if (!(pair in pseen)) { pseen[pair] = 1; porder[++np] = pair } }
  END {
    printf "%-16s", "pair (ms)"; for (i = 1; i <= ni; i++) printf " %12s", order[i]; printf "\n"
    for (p = 1; p <= np; p++) {
      printf "%-16s", porder[p]
      for (i = 1; i <= ni; i++) { v = best[order[i] SUBSEP porder[p]]; tot[i] += v; printf " %12.2f", v }
      printf "\n"
    }
    printf "%-16s", "TOTAL"; for (i = 1; i <= ni; i++) printf " %12.2f", tot[i]; printf "\n"
    printf "%-16s", "speedup vs " order[1]; for (i = 1; i <= ni; i++) printf " %11.2fx", tot[1] / tot[i]; printf "\n"
    n = 0; for (p in bad) { printf "DISTANCE MISMATCH %s: %s\n", p, bad[p]; n++ }
    if (n == 0) printf "distances: all implementations agree on every pair\n"
  }' "$out" | tee "$work/report.txt"

if [ -n "${SAVE:-}" ]; then
  host=$(hostname -s 2>/dev/null | tr -c 'A-Za-z0-9._-' '_' | cut -c1-40)
  n=1
  while [ -e "bench/results/${host:-host}-run$n" ]; do n=$((n + 1)); done
  dest="bench/results/${host:-host}-run$n"
  mkdir -p "$dest"
  cp "$work/env.txt" "$work/raw.txt" "$work/report.txt" "$dest/"
  echo "saved to $dest" >&2
fi
! grep -q '^DISTANCE MISMATCH' "$work/report.txt"
