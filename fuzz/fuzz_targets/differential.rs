//! APTED against the independent Zhang-Shasha oracle from the test suite and,
//! on tiny trees, against the brute-force algorithm.
#![no_main]

use apted::{AllPossibleMappingsTED, APTED};
use apted_fuzz::{close, decode_cost_model, decode_tree, Bytes};
use libfuzzer_sys::fuzz_target;

#[path = "../../tests/oracle/mod.rs"]
mod oracle;

fuzz_target!(|data: &[u8]| {
    let mut b = Bytes(data);
    let (cm, _, _) = decode_cost_model(&mut b);
    let alphabet = 1 + b.byte() % 4;
    let t1 = decode_tree(&mut b, 24, alphabet);
    let t2 = decode_tree(&mut b, 24, alphabet);
    let expected = oracle::zhang_shasha(&cm, &t1, &t2);
    let d = APTED::new(cm).compute_edit_distance(&t1, &t2);
    assert!(
        close(d, expected),
        "apted {d} != zhang-shasha {expected}\n{t1}\n{t2}"
    );
    let l = APTED::new(cm).compute_edit_distance_spf_test(&t1, &t2, 0);
    let r = APTED::new(cm).compute_edit_distance_spf_test(&t1, &t2, 1);
    assert!(
        close(l, expected),
        "spfL {l} != zhang-shasha {expected}\n{t1}\n{t2}"
    );
    assert!(
        close(r, expected),
        "spfR {r} != zhang-shasha {expected}\n{t1}\n{t2}"
    );

    let s1 = decode_tree(&mut b, 5, alphabet);
    let s2 = decode_tree(&mut b, 5, alphabet);
    let bf = AllPossibleMappingsTED::new(cm).compute_edit_distance(&s1, &s2);
    let d = APTED::new(cm).compute_edit_distance(&s1, &s2);
    assert!(close(d, bf), "apted {d} != brute force {bf}\n{s1}\n{s2}");
});
