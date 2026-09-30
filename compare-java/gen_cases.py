#!/usr/bin/env python3
"""Generates the input of the Java differential run: ordered tree pairs under
eight cost models, written as `id<TAB>kind<TAB>t1<TAB>t2<TAB>model` lines.

  gen_cases.py REPO SHAPES_DIR OUT            full run (about 45,000 cases)
  gen_cases.py REPO SHAPES_DIR OUT --golden   the subset kept as golden files
                                              (`kind t1 t2 model`, no id)

REPO is this repository (for the JSON fixtures); SHAPES_DIR holds the nine
benchmark pairs written by `cargo run --release --example bench -- --dump
SHAPES_DIR N` (N=1000 for the full run, N=200 for the golden subset).
Deterministic: fixed seed, Python 3 `random`.
"""
import json, random, sys, os
R = random.Random(20260930)
MODELS = ["unit", "0.4,0.4,0.6", "0.1,0.2,0.3", "1,2,0.5", "3,1,5", "1.3,0.17,0.91", "0.7,0.3,1.9", "1,0.1,0.5"]
ALPHA = "abcdefghijklmnopqrstuvwxyz0123456789"
cases = []  # (kind, t1, t2, model)
pairs = []  # (kind, first case index) per tree pair, for the golden subset
def add(kind, t1, t2, models=MODELS):
    pairs.append((kind, len(cases)))
    for m in models:
        cases.append((kind, t1, t2, m))

def to_s(parent, labels):
    n = len(parent)
    kids = [[] for _ in range(n)]
    for i in range(1, n): kids[parent[i]].append(i)
    out = []
    # iterative preorder emit
    stack = [(0, 0)]
    while stack:
        v, k = stack.pop()
        if k == 0: out.append("{" + labels[v])
        if k < len(kids[v]):
            stack.append((v, k + 1)); stack.append((kids[v][k], 0))
        else:
            out.append("}")
    return "".join(out)

def rand_tree(n, alpha, style=None):
    style = style or R.choice(["uniform", "deep", "flat", "uniform"])
    parent = [0] * n
    for i in range(1, n):
        if style == "uniform": parent[i] = R.randrange(i)
        elif style == "deep": parent[i] = R.randrange(max(0, i - 3), i)
        else: parent[i] = R.randrange(min(i, 3))
    labels = [R.choice(alpha) for _ in range(n)]
    return parent, labels

def mutate(parent, labels, alpha, edits):
    parent = parent[:]; labels = labels[:]
    for _ in range(edits):
        op = R.randrange(3)
        if op == 0 or len(parent) == 1:  # insert leaf
            parent.append(R.randrange(len(parent))); labels.append(R.choice(alpha))
            # keep parent<child order: new node is last, fine
        elif op == 1:  # relabel
            labels[R.randrange(len(labels))] = R.choice(alpha)
        else:  # delete a leaf
            ch = set(parent[1:])
            leaves = [i for i in range(1, len(parent)) if i not in ch]
            if leaves and leaves[-1] == len(parent) - 1:
                parent.pop(); labels.pop()
    return parent, labels

# 1 fixtures
for f in ["correctness_test_cases.json", "mini.json"]:
    d = json.load(open(os.path.join(sys.argv[1], "tests/resources", f)))
    for c in d:
        add("fixture", c["t1"], c["t2"])
# 2 random small
for i in range(5000):
    alpha = R.sample(ALPHA, R.randint(1, 6))
    n1 = R.randint(1, 40)
    p1, l1 = rand_tree(n1, alpha)
    if R.random() < 0.3:
        p2, l2 = mutate(p1, l1, alpha, R.randint(1, 5))
    else:
        p2, l2 = rand_tree(R.randint(1, 40), alpha)
    add("small", to_s(p1, l1), to_s(p2, l2))
# 3 large
for i in range(200):
    alpha = R.sample(ALPHA, R.randint(2, 8))
    p1, l1 = rand_tree(R.randint(100, 300), alpha)
    if R.random() < 0.3:
        p2, l2 = mutate(p1, l1, alpha, R.randint(5, 40))
    else:
        p2, l2 = rand_tree(R.randint(100, 300), alpha)
    add("large", to_s(p1, l1), to_s(p2, l2))
# shapes
for name in ["random-random","left-left","right-right","binary-binary","zigzag-zigzag","flat-flat","left-right","zigzag-binary","random-zigzag"]:
    a, b = open(os.path.join(sys.argv[2], name + ".txt")).read().split("\n")[:2]
    add("shape:" + name, a, b)
# targeted: single node vs every ordered tree shape up to 5 nodes
def shapes(n):
    # all ordered trees with n nodes as parent arrays (preorder)
    res = []
    def rec(parent, path):
        if len(parent) == n: res.append(parent[:]); return
        for d in range(len(path)):
            v = len(parent)
            parent.append(path[d]); rec(parent, path[:d + 1] + [v]); parent.pop()
    rec([0], [0])
    return res
for n in range(1, 6):
    for par in shapes(n):
        for _ in range(2):
            labs = [R.choice("abc") for _ in range(n)]
            s = to_s(par, labs)
            for x in "abc":
                add("single", "{" + x + "}", s)
                add("single", s, "{" + x + "}")
add("single", "{b}", "{c{a}}")
add("single", "{c{a}}", "{b}")
golden = "--golden" in sys.argv
if golden:
    G5 = ["unit", "0.4,0.4,0.6", "1.3,0.17,0.91", "0.7,0.3,1.9", "1,0.1,0.5"]
    keep = []
    by_kind = {}
    for kind, first in pairs:
        by_kind.setdefault(kind.split(":")[0], []).append((kind, first))
    for kind, first in by_kind["fixture"]:
        keep += [j for j in range(first, first + len(MODELS)) if cases[j][3] in G5]
    keep += [j for _, first in by_kind["single"] for j in range(first, first + len(MODELS))]
    for n, (kind, first) in enumerate(by_kind["small"]):
        if n % 8 == 0:
            keep.append(first + (n // 8) % len(MODELS))
    for n, (kind, first) in enumerate(by_kind["large"]):
        if n % 50 == 0:
            keep += [j for j in range(first, first + len(MODELS)) if cases[j][3] in ("unit", "1.3,0.17,0.91")]
    for kind, first in by_kind["shape"]:
        keep += [j for j in range(first, first + len(MODELS)) if cases[j][3] in ("unit", "1.3,0.17,0.91")]
    with open(sys.argv[3], "w") as f:
        for j in sorted(keep):
            kind, t1, t2, m = cases[j]
            f.write(f"{kind}\t{t1}\t{t2}\t{m}\n")
    print(len(keep), "golden cases")
else:
    with open(sys.argv[3], "w") as f:
        for i, (kind, t1, t2, m) in enumerate(cases):
            assert "\t" not in t1 + t2
            f.write(f"{i}\t{kind}\t{t1}\t{t2}\t{m}\n")
from collections import Counter
print(len(cases), Counter(c[0].split(":")[0] for c in cases))
