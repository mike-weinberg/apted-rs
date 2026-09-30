//! Regression tests for the security review: hostile or extreme input must
//! give a result or an error, never a stack overflow, an abort or an
//! out-of-range panic.

use apted::{
    BracketStringInputParser, CostModel, Node, StringNodeData, StringUnitCostModel, TedError, APTED,
};

fn leaf(l: &str) -> Node<StringNodeData> {
    Node::new(StringNodeData::new(l))
}

/// A chain of `depth` nodes, each the only child of the previous one.
fn chain(depth: usize, label: &str) -> Node<StringNodeData> {
    let mut t = leaf(label);
    for _ in 1..depth {
        let mut p = leaf(label);
        p.add_child(t);
        t = p;
    }
    t
}

/// A spine of `depth` nodes where each spine node also has a leaf child on
/// alternating sides, which makes GTED descend through every level.
fn zigzag(depth: usize) -> Node<StringNodeData> {
    let mut t = leaf("s");
    for i in 0..depth {
        let mut p = leaf("s");
        if i % 2 == 0 {
            p.add_child(leaf("l"));
            p.add_child(t);
        } else {
            p.add_child(t);
            p.add_child(leaf("l"));
        }
        t = p;
    }
    t
}

/// Runs `f` on a thread with a deliberately small stack: every tree walk
/// must be iterative for this to pass.
fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn deep_chain_is_stack_safe() {
    let d = on_small_stack(|| {
        let t1 = chain(200_000, "a");
        let t2 = leaf("a");
        let d = APTED::new(StringUnitCostModel).compute_edit_distance(&t1, &t2);
        let copy = t1.clone();
        assert!(copy == t1);
        assert_eq!(t1.node_count(), 200_000);
        d
    });
    assert_eq!(d, 199_999.0);
}

#[test]
fn deep_decomposition_is_stack_safe() {
    let (d, mapping_len) = on_small_stack(|| {
        let t1 = zigzag(200);
        let t2 = zigzag(199);
        let mut apted = APTED::new(StringUnitCostModel);
        let d = apted.compute_edit_distance(&t1, &t2);
        (d, apted.compute_edit_mapping().len())
    });
    assert_eq!(d, 2.0);
    assert!(mapping_len <= 401 + 399);
}

#[test]
fn deep_bracket_string_is_stack_safe() {
    let depth = 100_000;
    let s = "{a".repeat(depth) + &"}".repeat(depth);
    let t = on_small_stack(move || {
        BracketStringInputParser::new()
            .try_from_string(&s)
            .map(|t| t.node_count())
    });
    assert_eq!(t, Ok(depth));
}

#[test]
fn wide_tree_cost_sums_do_not_overflow() {
    // i32 cost sums overflow above ~46k children of one node.
    let mut star = leaf("r");
    for _ in 0..60_000 {
        star.add_child(leaf("x"));
    }
    let mut small = leaf("r");
    small.add_child(leaf("x"));
    let d = APTED::new(StringUnitCostModel).compute_edit_distance(&star, &small);
    assert_eq!(d, 59_999.0);
}

struct NanDelete;
impl CostModel<StringNodeData> for NanDelete {
    fn del(&self, _: &Node<StringNodeData>) -> f32 {
        f32::NAN
    }
    fn ins(&self, _: &Node<StringNodeData>) -> f32 {
        1.0
    }
    fn ren(&self, a: &Node<StringNodeData>, b: &Node<StringNodeData>) -> f32 {
        if a.node_data().same_label(b.node_data()) {
            0.0
        } else {
            1.0
        }
    }
}

#[test]
fn nan_costs_give_an_error_not_a_panic() {
    let p = BracketStringInputParser::new();
    let (t1, t2) = (p.from_string("{a{b}}"), p.from_string("{a}"));
    let mut apted = APTED::new(NanDelete);
    assert!(apted.compute_edit_distance(&t1, &t2).is_nan());
    assert_eq!(apted.try_compute_edit_mapping(), Err(TedError::NotANumber));
}

#[test]
fn mapping_before_distance_is_an_error() {
    let p = BracketStringInputParser::new();
    let (t1, t2) = (p.from_string("{a{b}}"), p.from_string("{a}"));
    let (u1, u2) = (p.from_string("{x{y}{z}}"), p.from_string("{x}"));
    let mut apted = APTED::new(StringUnitCostModel);
    assert_eq!(
        apted.try_compute_edit_mapping(),
        Err(TedError::DistanceNotComputed)
    );
    apted.compute_edit_distance(&t1, &t2);
    assert!(apted.try_compute_edit_mapping().is_ok());
    // A new pair invalidates the previous distance.
    apted.init(&u1, &u2);
    assert_eq!(
        apted.try_compute_edit_mapping(),
        Err(TedError::DistanceNotComputed)
    );
}

#[test]
fn memory_limit_is_enforced_before_work() {
    let t = zigzag(100);
    let mut apted = APTED::new(StringUnitCostModel).with_memory_limit(1024);
    assert!(matches!(
        apted.try_compute_edit_distance(&t, &t),
        Err(TedError::MemoryLimitExceeded { .. })
    ));
    let mut apted = APTED::new(StringUnitCostModel).with_memory_limit(1 << 30);
    assert_eq!(apted.try_compute_edit_distance(&t, &t), Ok(0.0));
}

#[test]
fn malformed_input_is_an_error() {
    let p = BracketStringInputParser::new();
    for bad in ["{a}{b}", "{a{b}x{c}}", "{a{b}", "junk{a}", "{a<{b}>}"] {
        assert!(p.try_from_string(bad).is_err(), "accepted {bad:?}");
    }
}
