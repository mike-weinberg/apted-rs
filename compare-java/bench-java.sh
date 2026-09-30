#!/usr/bin/env bash
# EXTRA_IMPL hook for bench/run_all.sh: times the upstream Java APTED.
# Normally started through compare-java/bench.sh.
#
#   bench-java.sh setup WORK             clone upstream at the pinned commit
#   bench-java.sh run TREES REPS WORK    print AptedBench output
#   bench-java.sh env WORK               print the Java environment
#   bench-java.sh cleanup WORK           remove the clone and compiled classes
#
# Environment:
#   JAVA_DOCKER=CTX   compile and run inside docker (`docker --context CTX`),
#                     streaming sources and trees over stdin. Unset: host JDK.
#   JAVA_IMAGE=IMG    image for JAVA_DOCKER (default eclipse-temurin:21-jdk)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
JAVA_REPO=https://github.com/DatabaseGroup/apted.git
JAVA_COMMIT=193666ba2b68aa8106dc2d1d0a78b8a0e07c286b
JAVA_OPTS=(-Xss64m -Xmx8g)
JAVA_WARMUP=2
JAVA_DOCKER=${JAVA_DOCKER:-}
JAVA_IMAGE=${JAVA_IMAGE:-eclipse-temurin:21-jdk}
# Git must not run hooks or an fsmonitor from a reused clone's config.
safe_git() { git -c core.hooksPath=/dev/null -c core.fsmonitor=false "$@"; }

cmd=${1:?usage: bench-java.sh setup|run|env|cleanup ...}
case "$cmd" in
  setup)
    work=$2
    [ -d "$work/java-src" ] || safe_git clone -q --no-checkout "$JAVA_REPO" "$work/java-src"
    # A reused clone may carry local edits or extra files: force the pinned
    # commit, delete everything untracked and verify.
    safe_git -C "$work/java-src" checkout -q --force --detach "$JAVA_COMMIT"
    safe_git -C "$work/java-src" clean -q -fdx
    if [ "$(safe_git -C "$work/java-src" rev-parse HEAD)" != "$JAVA_COMMIT" ] ||
       [ -n "$(safe_git -C "$work/java-src" status --porcelain)" ]; then
      echo "java-src is not a clean checkout of $JAVA_COMMIT" >&2; exit 1
    fi
    if [ -z "$JAVA_DOCKER" ]; then
      rm -rf "$work/java-classes"; mkdir -p "$work/java-classes"
      sources=()
      while IFS= read -r -d '' f; do sources+=("$f"); done \
        < <(find "$work/java-src/src/main/java/at" -name '*.java' -print0)
      javac -nowarn -d "$work/java-classes" "${sources[@]}" "$here/AptedBench.java" 2>&1 |
        grep -v -e '^Note:' -e '^Picked up' >&2 || true
    fi
    ;;
  run)
    trees=$2; reps=$3; work=$4
    if [ -z "$JAVA_DOCKER" ]; then
      java "${JAVA_OPTS[@]}" -cp "$work/java-classes" AptedBench "$trees" "$reps" "$JAVA_WARMUP" \
        2> >(grep -v '^Picked up' >&2)
    else
      # Nothing is mounted: sources, harness and trees travel in one tar stream.
      COPYFILE_DISABLE=1 tar -c -C "$work/java-src" src/main/java/at -C "$here" AptedBench.java \
        -C "$(dirname "$trees")" "$(basename "$trees")" |
        docker --context "$JAVA_DOCKER" run --rm -i "$JAVA_IMAGE" sh -c '
          mkdir /w && cd /w && tar xf - 2>/dev/null
          find . -name "._*" -delete
          mkdir cls && javac -nowarn -d cls $(find src -name "*.java") AptedBench.java 2>&1 | grep -v "^Note:" >&2
          exec java '"${JAVA_OPTS[*]}"' -cp cls AptedBench "'"$(basename "$trees")"'" '"$reps $JAVA_WARMUP"
    fi
    ;;
  env)
    work=$2
    echo
    echo "## Java"
    if [ -z "$JAVA_DOCKER" ]; then
      java -version 2>&1 | grep -v '^Picked up'
    else
      echo "docker context: $JAVA_DOCKER, image: $JAVA_IMAGE"
      docker --context "$JAVA_DOCKER" run --rm "$JAVA_IMAGE" java -version 2>&1 | grep -v '^Picked up'
    fi
    echo "java: ${JAVA_OPTS[*]}, $JAVA_WARMUP untimed warm-up runs per pair"
    echo "upstream Java APTED: $(safe_git -C "$work/java-src" rev-parse HEAD)"
    ;;
  cleanup)
    rm -rf "$2/java-src" "$2/java-classes"
    ;;
  *) echo "unknown command: $cmd" >&2; exit 2 ;;
esac
