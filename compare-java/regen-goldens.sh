#!/usr/bin/env bash
# Regenerates the Java column of tests/resources/java_golden/cases.tsv: runs
# the pinned upstream Java APTED on the inputs already recorded in that file
# (its first four columns) and writes the outputs next to them.
#
# Usage: compare-java/regen-goldens.sh [--check]
#   --check   compare with the committed file, change nothing
# Environment:
#   JAVA_DOCKER=CTX   compile and run in `docker --context CTX` (sources and
#                     inputs stream over stdin, nothing is mounted). Unset:
#                     host JDK (javac, java).
#   JAVA_IMAGE=IMG    image for JAVA_DOCKER (default eclipse-temurin:21-jdk)
# Needs: git, python3, tar, and a JDK 11+ or docker. Network: one clone of
# github.com/DatabaseGroup/apted into compare-java/ref-java (gitignored).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
golden="$here/../tests/resources/java_golden/cases.tsv"
JAVA_REPO=https://github.com/DatabaseGroup/apted.git
JAVA_COMMIT=193666ba2b68aa8106dc2d1d0a78b8a0e07c286b
JAVA_DOCKER=${JAVA_DOCKER:-}
JAVA_IMAGE=${JAVA_IMAGE:-eclipse-temurin:21-jdk}
safe_git() { git -c core.hooksPath=/dev/null -c core.fsmonitor=false "$@"; }

src="$here/ref-java"
[ -d "$src" ] || safe_git clone -q --no-checkout "$JAVA_REPO" "$src"
safe_git -C "$src" checkout -q --force --detach "$JAVA_COMMIT"
safe_git -C "$src" clean -q -fdx
[ "$(safe_git -C "$src" rev-parse HEAD)" = "$JAVA_COMMIT" ] || { echo "ref-java is not $JAVA_COMMIT" >&2; exit 1; }

tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
# id<TAB>kind<TAB>t1<TAB>t2<TAB>model, from the first four columns.
cut -f1-4 "$golden" | awk -F'\t' -v OFS='\t' '{ print NR - 1, $0 }' >"$tmp/input.tsv"

if [ -z "$JAVA_DOCKER" ]; then
  mkdir "$tmp/cls"
  mapfile -d '' sources < <(find "$src/src/main/java/at" -name '*.java' -print0)
  javac -nowarn -d "$tmp/cls" "${sources[@]}" "$here/GoldenDump.java" 2>&1 | grep -v -e '^Note:' -e '^Picked up' >&2 || true
  java -Xss512m -cp "$tmp/cls" GoldenDump <"$tmp/input.tsv" >"$tmp/java.out"
else
  COPYFILE_DISABLE=1 tar -c -C "$src" src/main/java/at -C "$here" GoldenDump.java -C "$tmp" input.tsv |
    docker --context "$JAVA_DOCKER" run --rm -i "$JAVA_IMAGE" sh -c '
      mkdir /w && cd /w && tar xf - 2>/dev/null
      find . -name "._*" -delete
      mkdir cls && javac -nowarn -d cls $(find src -name "*.java") GoldenDump.java 2>&1 | grep -v "^Note:" >&2
      exec java -Xss512m -cp cls GoldenDump <input.tsv' >"$tmp/java.out"
fi

# kind t1 t2 model d drev spfL spfR mc map apm ("-" when a field is absent)
python3 - "$tmp/input.tsv" "$tmp/java.out" >"$tmp/cases.tsv" <<'PY'
import sys
inputs = [l.rstrip("\n").split("\t") for l in open(sys.argv[1])]
outs = {}
for l in open(sys.argv[2]):
    p = l.split()
    outs[int(p[0])] = dict(kv.split("=", 1) for kv in p[1:])
assert len(outs) == len(inputs), (len(outs), len(inputs))
for i, (_, kind, t1, t2, model) in enumerate(inputs):
    o = outs[i]
    print("\t".join([kind, t1, t2, model, o["d"], o["drev"], o["spfL"], o["spfR"], o["mc"], o["map"], o.get("apm", "-")]))
PY

if [ "${1:-}" = "--check" ]; then
  cmp "$tmp/cases.tsv" "$golden" && echo "golden file matches the Java output"
else
  cp "$tmp/cases.tsv" "$golden"
  echo "wrote $golden ($(wc -l <"$golden" | tr -d ' ') cases)"
fi
