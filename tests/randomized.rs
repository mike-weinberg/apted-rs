//! Differential tests on generated trees. Not part of the Java suite: they
//! guard the port's performance work by comparing APTED against an
//! independent Zhang-Shasha implementation and the brute-force algorithm.

mod oracle;

use apted::{
    AllPossibleMappingsTED, CostModel, Node, PerEditOperationStringNodeDataCostModel,
    StringNodeData, StringUnitCostModel, APTED,
};

use oracle::{check_mapping, zhang_shasha};

/// Small deterministic PRNG (xorshift64*), so the test needs no dependency.
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
}

/// Random tree with `size` nodes and labels drawn from `labels` letters.
/// Each new node attaches to a random existing node, which yields a mix of
/// deep and bushy shapes.
fn random_tree(rng: &mut Rng, size: usize, labels: usize) -> Node<StringNodeData> {
    let mut parent = vec![usize::MAX; size];
    for (i, p) in parent.iter_mut().enumerate().skip(1) {
        // Bias towards recent nodes to get deeper trees sometimes.
        *p = if rng.below(2) == 0 {
            i - 1 - rng.below(i.min(3))
        } else {
            rng.below(i)
        };
    }
    let label = |rng: &mut Rng| ((b'a' + rng.below(labels) as u8) as char).to_string();
    let mut nodes: Vec<Option<Node<StringNodeData>>> = (0..size)
        .map(|_| Some(Node::new(StringNodeData::new(label(rng)))))
        .collect();
    // Children get higher indices than parents, so attach in reverse and
    // then reverse each child list to keep insertion order.
    for i in (1..size).rev() {
        let child = nodes[i].take().unwrap();
        nodes[parent[i]].as_mut().unwrap().add_child(child);
    }
    reverse_children(nodes[0].take().unwrap())
}

fn reverse_children(n: Node<StringNodeData>) -> Node<StringNodeData> {
    let mut out = Node::new(n.node_data().clone());
    for c in n.children().iter().rev() {
        out.add_child(reverse_children(c.clone()));
    }
    out
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-3 * (1.0 + a.abs().max(b.abs()))
}

/// Checks every APTED entry point against the Zhang-Shasha distance.
fn check_all<C: CostModel<StringNodeData> + Copy>(
    cm: C,
    t1: &Node<StringNodeData>,
    t2: &Node<StringNodeData>,
) -> Result<(), String> {
    let expected = zhang_shasha(&cm, t1, t2);
    let mut apted = APTED::new(cm);
    let d = apted.compute_edit_distance(t1, t2);
    let mapping = apted.compute_edit_mapping();
    let mc = apted.mapping_cost(&mapping);
    let d_rev = APTED::new(cm).compute_edit_distance(t2, t1);
    let d_l = APTED::new(cm).compute_edit_distance_spf_test(t1, t2, 0);
    let d_r = APTED::new(cm).compute_edit_distance_spf_test(t1, t2, 1);
    for (what, v) in [("apted", d), ("mapping", mc), ("spfL", d_l), ("spfR", d_r)] {
        if !close(expected, v) {
            return Err(format!(
                "{what}: expected {expected}, got {v}\n  t1={t1}\n  t2={t2}"
            ));
        }
    }
    let expected_rev = zhang_shasha(&cm, t2, t1);
    if !close(expected_rev, d_rev) {
        return Err(format!(
            "reversed: expected {expected_rev}, got {d_rev}\n  t1={t1}\n  t2={t2}"
        ));
    }
    Ok(())
}

#[test]
fn random_trees_match_zhang_shasha_unit_cost() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..400 {
        let (s1, s2) = (1 + rng.below(40), 1 + rng.below(40));
        let labels = 1 + rng.below(5);
        let t1 = random_tree(&mut rng, s1, labels);
        let t2 = random_tree(&mut rng, s2, labels);
        check_all(StringUnitCostModel, &t1, &t2).unwrap();
    }
}

#[test]
fn random_trees_match_zhang_shasha_per_edit_operation_cost() {
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    let costs = [
        (0.4, 0.4, 0.6),
        (1.0, 2.0, 0.5),
        (3.0, 1.0, 5.0),
        (0.7, 0.3, 1.9),
    ];
    for k in 0..400 {
        let (d, i, r) = costs[k % costs.len()];
        let cm = PerEditOperationStringNodeDataCostModel::new(d, i, r);
        let (s1, s2) = (1 + rng.below(40), 1 + rng.below(40));
        let labels = 1 + rng.below(5);
        let t1 = random_tree(&mut rng, s1, labels);
        let t2 = random_tree(&mut rng, s2, labels);
        check_all(cm, &t1, &t2).unwrap();
    }
}

#[test]
fn larger_random_trees_match_zhang_shasha() {
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    for _ in 0..20 {
        let (s1, s2) = (100 + rng.below(200), 100 + rng.below(200));
        let t1 = random_tree(&mut rng, s1, 4);
        let t2 = random_tree(&mut rng, s2, 4);
        check_all(StringUnitCostModel, &t1, &t2).unwrap();
    }
}

#[test]
fn tiny_random_trees_match_brute_force() {
    let mut rng = Rng(0xCAFE_F00D_1234_5678);
    let cm = PerEditOperationStringNodeDataCostModel::new(0.4, 0.4, 0.6);
    for _ in 0..150 {
        let (s1, s2) = (1 + rng.below(5), 1 + rng.below(5));
        let t1 = random_tree(&mut rng, s1, 3);
        let t2 = random_tree(&mut rng, s2, 3);
        let d = APTED::new(cm).compute_edit_distance(&t1, &t2);
        let bf = AllPossibleMappingsTED::new(cm).compute_edit_distance(&t1, &t2);
        assert!(close(d, bf), "expected {bf}, got {d}\n  t1={t1}\n  t2={t2}");
    }
}

#[test]
fn edit_mappings_are_valid_for_unit_and_per_edit_operation_cost() {
    let mut rng = Rng(0x5EED_0FA1_1BEE_F001);
    let per_edit = [
        PerEditOperationStringNodeDataCostModel::new(0.7, 0.3, 1.9),
        PerEditOperationStringNodeDataCostModel::new(0.4, 0.4, 0.6),
        PerEditOperationStringNodeDataCostModel::new(1.3, 0.17, 0.91),
    ];
    for k in 0..600 {
        let (s1, s2) = (1 + rng.below(40), 1 + rng.below(40));
        let labels = 1 + rng.below(5);
        let t1 = random_tree(&mut rng, s1, labels);
        let t2 = random_tree(&mut rng, s2, labels);
        check_mapping(StringUnitCostModel, &t1, &t2).unwrap();
        check_mapping(per_edit[k % per_edit.len()], &t1, &t2).unwrap();
    }
    for _ in 0..10 {
        let (s1, s2) = (100 + rng.below(100), 100 + rng.below(100));
        let t1 = random_tree(&mut rng, s1, 4);
        let t2 = random_tree(&mut rng, s2, 4);
        check_mapping(StringUnitCostModel, &t1, &t2).unwrap();
        check_mapping(per_edit[0], &t1, &t2).unwrap();
    }
}
