#!/usr/bin/env bash
# Prints the benchmark environment: hardware, OS, load,
# toolchains and the exact source revisions. run_all.sh saves this next to
# every result so numbers are never separated from the machine they came from.
# Usage: bench/env.sh
set -uo pipefail
cd "$(dirname "$0")/.."

section() { printf '\n## %s\n' "$1"; }
try() { "$@" 2>/dev/null || echo "(unavailable: $*)"; }

echo "# Benchmark environment"

section "CPU"
if command -v lscpu >/dev/null; then
  lscpu | grep -E '^(Architecture|Model name|CPU\(s\)|Thread\(s\) per core|Core\(s\) per socket|Socket\(s\)|CPU max MHz|CPU MHz|BogoMIPS|L1d cache|L2 cache|L3 cache|Hypervisor vendor|Virtualization type):'
  echo "SIMD: $(grep -o -w -E 'sse4_2|avx2|avx512f|neon|asimd' /proc/cpuinfo 2>/dev/null | sort -u | tr '\n' ' ')"
else
  try sysctl -n machdep.cpu.brand_string
  echo "cpus: $(try sysctl -n hw.ncpu)"
fi
gov=/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor
[ -r "$gov" ] && echo "scaling governor: $(cat "$gov")" || echo "scaling governor: (not exposed)"

section "Memory"
if command -v free >/dev/null; then free -h | head -2; else echo "$(try sysctl -n hw.memsize) bytes"; fi

section "OS and virtualization"
uname -srm
[ -r /etc/os-release ] && grep -E '^PRETTY_NAME=' /etc/os-release
command -v systemd-detect-virt >/dev/null && echo "virt: $(systemd-detect-virt 2>/dev/null)"
[ -f /.dockerenv ] && echo "container: docker"
[ -r /proc/1/cgroup ] && grep -q -E 'docker|kubepods|containerd' /proc/1/cgroup && echo "container: cgroup indicates container"
if [ -r /sys/fs/cgroup/cpu.max ]; then echo "cgroup cpu.max: $(cat /sys/fs/cgroup/cpu.max)"; fi
echo "online cpus (nproc): $(try nproc)"

section "Load at capture"
uptime

section "Toolchains"
# Release dates are dropped from the version lines.
try rustc -Vv | grep -E '^(rustc|host|LLVM)' | sed -E 's/ [0-9]{4}-[0-9]{2}-[0-9]{2}\)/)/; s/ \([0-9a-f]{9}\)//'
try cargo -V | sed -E 's/ \([0-9a-f]{9}\)//; s/ \([0-9a-f]{9} [0-9-]{10}\)//'

section "Sources"
echo "apted-rs HEAD: $(git rev-parse HEAD) ($(git log -1 --format=%s))"
git diff --quiet HEAD -- . 2>/dev/null && echo "working tree: clean" || echo "working tree: MODIFIED"
