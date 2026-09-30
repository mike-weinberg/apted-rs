#!/usr/bin/env python3
"""Compares two outputs of GoldenDump.java / rust-driver field by field.

  compare.py A.out B.out [--floats]

Prints, per field, how many cases are bit-identical, then the ids of the
differing cases. With --floats, each differing distance is also shown as a
float. Exit status 1 when any field differs.
"""
import struct, sys

FIELDS = ["d", "drev", "spfL", "spfR", "mc", "map", "apm"]


def load(path):
    out = {}
    for line in open(path):
        parts = line.split()
        out[int(parts[0])] = dict(kv.split("=", 1) for kv in parts[1:])
    return out


def fl(h):
    return struct.unpack(">f", bytes.fromhex(h))[0]


a, b = load(sys.argv[1]), load(sys.argv[2])
if set(a) != set(b):
    sys.exit(f"case ids differ: {len(a)} vs {len(b)} cases")
bad = False
for k in FIELDS:
    ids = [i for i in a if k in a[i] or k in b[i]]
    diff = [i for i in ids if a[i].get(k) != b[i].get(k)]
    print(f"{k:5s} {len(ids) - len(diff)}/{len(ids)} identical")
    for i in diff[:20]:
        extra = ""
        if "--floats" in sys.argv and k not in ("map",) and "-" not in (a[i][k], b[i][k]):
            extra = f"  {fl(a[i][k])!r} vs {fl(b[i][k])!r}"
        print(f"  id {i}{extra}")
    bad |= bool(diff)
sys.exit(1 if bad else 0)
