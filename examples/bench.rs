//! Benchmark over tree shapes that stress different strategy paths.
//!
//! Usage: `cargo run --release --example bench [size] [reps] [filter]`
//! or `... --example bench --dump DIR [size]` to write each pair's trees in
//! bracket notation to `DIR/<pair>.txt` (one tree per line), for running
//! other implementations on identical input.
//! Prints the median time per pair and the computed distance, which must
//! stay identical across optimizations.

use std::time::Instant;

use apted::{Node, StringNodeData, StringUnitCostModel, APTED};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn label(&mut self) -> StringNodeData {
        StringNodeData::new(((b'a' + self.below(20) as u8) as char).to_string())
    }
}

/// Builds a tree from a parent array where parents precede children.
fn from_parents(rng: &mut Rng, parent: &[usize]) -> Node<StringNodeData> {
    let n = parent.len();
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 1..n {
        kids[parent[i]].push(i);
    }
    let labels: Vec<StringNodeData> = (0..n).map(|_| rng.label()).collect();
    // Build bottom-up without recursion; children have larger indices.
    let mut built: Vec<Option<Node<StringNodeData>>> = (0..n).map(|_| None).collect();
    for i in (0..n).rev() {
        let mut node = Node::new(labels[i].clone());
        for &c in &kids[i] {
            node.add_child(built[c].take().unwrap());
        }
        built[i] = Some(node);
    }
    built[0].take().unwrap()
}

fn shape(name: &str, n: usize, rng: &mut Rng) -> Node<StringNodeData> {
    let mut parent = vec![0usize; n];
    match name {
        "random" => {
            for (i, p) in parent.iter_mut().enumerate().skip(1) {
                *p = rng.below(i);
            }
        }
        // Left branch: a spine going left, each spine node with a leaf on
        // its right.
        "left" => {
            let mut spine = 0;
            let mut i = 1;
            while i < n {
                parent[i] = spine;
                if i + 1 < n {
                    parent[i + 1] = spine;
                }
                spine = i;
                i += 2;
            }
        }
        // Right branch: the spine continues through the last child.
        "right" => {
            let mut spine = 0;
            let mut i = 1;
            while i < n {
                parent[i] = spine;
                if i + 1 < n {
                    parent[i + 1] = spine;
                    spine = i + 1;
                } else {
                    spine = i;
                }
                i += 2;
            }
        }
        "binary" => {
            for (i, p) in parent.iter_mut().enumerate().skip(1) {
                *p = (i - 1) / 2;
            }
        }
        // Zigzag: the spine alternates between first and last child.
        "zigzag" => {
            let mut spine = 0;
            let mut i = 1;
            let mut k = 0;
            while i < n {
                parent[i] = spine;
                if i + 1 < n {
                    parent[i + 1] = spine;
                    spine = if k % 2 == 0 { i } else { i + 1 };
                } else {
                    spine = i;
                }
                k += 1;
                i += 2;
            }
        }
        "flat" => {}
        _ => panic!("unknown shape {name}"),
    }
    from_parents(rng, &parent)
}

const PAIRS: [(&str, &str); 9] = [
    ("random", "random"),
    ("left", "left"),
    ("right", "right"),
    ("binary", "binary"),
    ("zigzag", "zigzag"),
    ("flat", "flat"),
    ("left", "right"),
    ("zigzag", "binary"),
    ("random", "zigzag"),
];

/// The pair's trees; the seed depends only on `n`, as in the benchmark loop.
fn pair_trees(a: &str, b: &str, n: usize) -> (Node<StringNodeData>, Node<StringNodeData>) {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ n as u64);
    let t1 = shape(a, n, &mut rng);
    let t2 = shape(b, n, &mut rng);
    (t1, t2)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--dump") {
        let dir = std::path::Path::new(&args[2]);
        let n: usize = args.get(3).map_or(1000, |s| s.parse().unwrap());
        std::fs::create_dir_all(dir).unwrap();
        for (a, b) in PAIRS {
            let (t1, t2) = pair_trees(a, b, n);
            std::fs::write(dir.join(format!("{a}-{b}.txt")), format!("{t1}\n{t2}\n")).unwrap();
        }
        return;
    }
    let n: usize = args.get(1).map_or(1000, |s| s.parse().unwrap());
    let reps: usize = args.get(2).map_or(5, |s| s.parse().unwrap());
    let filter = args.get(3).cloned().unwrap_or_default();
    let mut total = 0.0;
    for (a, b) in PAIRS {
        let name = format!("{a}-{b}");
        if !name.contains(&filter) {
            continue;
        }
        let (t1, t2) = pair_trees(a, b, n);
        let mut times = Vec::new();
        let mut d = 0.0;
        let mut subproblems = 0;
        for _ in 0..reps {
            let start = Instant::now();
            let mut apted = APTED::new(StringUnitCostModel);
            d = apted.compute_edit_distance(&t1, &t2);
            subproblems = apted.counter();
            times.push(start.elapsed().as_secs_f64() * 1e3);
        }
        times.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let median = times[times.len() / 2];
        total += median;
        println!(
            "{name:<16} n={n:<6} d={d:<8} median={median:>9.2} ms  {:>5.2} ns/subproblem",
            median * 1e6 / subproblems as f64
        );
    }
    println!("{:<16} total median={total:>9.2} ms", "ALL");
}
