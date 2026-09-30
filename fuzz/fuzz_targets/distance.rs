//! Distance on small trees decoded from the input, through the `try_` API
//! with a memory limit. Never panics; checks metric invariants and that the
//! edit mapping is valid (one-to-one, ancestor and sibling order kept) and
//! costs exactly the distance.
#![no_main]

use apted::{TedError, APTED};
use apted_fuzz::{close, decode_cost_model, decode_tree, Bytes};
use libfuzzer_sys::fuzz_target;

#[path = "../../tests/oracle/mod.rs"]
mod oracle;

const MAX_NODES: usize = 40;
const LIMIT: usize = 16 << 20;

fuzz_target!(|data: &[u8]| {
    let mut b = Bytes(data);
    let (cm, c, _r) = decode_cost_model(&mut b);
    let alphabet = 1 + b.byte() % 5;
    let t1 = decode_tree(&mut b, MAX_NODES, alphabet);
    let t2 = decode_tree(&mut b, MAX_NODES, alphabet);
    let t3 = decode_tree(&mut b, MAX_NODES, alphabet);
    let (n1, n2) = (t1.node_count() as f32, t2.node_count() as f32);

    let dist = |x, y| {
        APTED::new(cm)
            .with_memory_limit(LIMIT)
            .try_compute_edit_distance(x, y)
            .expect("small trees are within the limit")
    };
    // The lower bound below assumes delete and insert cost the same `c`, as in
    // `decode_cost_model`; widening the cost model needs a new bound.
    let d12 = dist(&t1, &t2);
    let d21 = dist(&t2, &t1);
    let d23 = dist(&t2, &t3);
    let d13 = dist(&t1, &t3);

    assert!(d12.is_finite() && d12 >= 0.0, "d12={d12}");
    assert!(close(dist(&t1, &t1), 0.0), "d(t,t) != 0");
    assert!(close(d12, d21), "asymmetric: {d12} vs {d21}\n{t1}\n{t2}");
    assert!(
        d12 <= c * (n1 + n2) + 1e-3,
        "above delete-all plus insert-all"
    );
    assert!(
        d12 + 1e-3 >= c * (n1 - n2).abs(),
        "below the size difference"
    );
    assert!(d13 <= d12 + d23 + 1e-3, "triangle: {d13} > {d12} + {d23}");

    // The limit is enforced: a one-byte limit must be refused, not computed.
    let refused = APTED::new(cm)
        .with_memory_limit(1)
        .try_compute_edit_distance(&t1, &t2);
    assert!(matches!(refused, Err(TedError::MemoryLimitExceeded { .. })));

    // Mapping: every node used once, ancestor and sibling order kept, and
    // the cost equals the distance.
    if let Err(e) = oracle::check_mapping(cm, &t1, &t2) {
        panic!("{e}");
    }
});
