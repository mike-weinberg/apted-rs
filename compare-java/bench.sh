#!/usr/bin/env bash
# Speed comparison with the upstream Java APTED: runs bench/run_all.sh with
# Java as the first column. Same arguments and environment as run_all.sh,
# plus JAVA_DOCKER and JAVA_IMAGE (see bench-java.sh).
#
# Usage: compare-java/bench.sh [rounds] [size] [reps]
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
EXTRA_IMPL="$here/bench-java.sh" EXTRA_LABEL=java exec "$here/../bench/run_all.sh" "$@"
