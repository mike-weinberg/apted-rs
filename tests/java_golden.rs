//! Pins this crate to recorded output of the upstream Java APTED.
//!
//! `tests/resources/java_golden/cases.tsv` holds inputs and the float bits,
//! mapping pair list and brute-force value the Java code produced (see the
//! README next to it for how). Every field must match bit for bit, with two
//! documented exceptions:
//!
//! - the `spf1` fix: the cells listed in `spf1_divergences.tsv` differ, and
//!   each must be Java undercounting (Rust is larger) with Rust equal to an
//!   independent Zhang-Shasha distance;
//! - the brute-force oracle: Java caps it at `size1 + size2`, the port does
//!   not.

mod oracle;

use std::collections::BTreeSet;
use std::path::PathBuf;

use apted::{
    AllPossibleMappingsTED, BracketStringInputParser, CostModel,
    PerEditOperationStringNodeDataCostModel, StringNodeData, StringUnitCostModel, APTED,
};

fn resource(name: &str) -> String {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests/resources/java_golden",
        name,
    ]
    .iter()
    .collect();
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// One golden line: the case and Java's output for it.
struct Golden<'a> {
    line: usize,
    kind: &'a str,
    t1: &'a str,
    t2: &'a str,
    model: &'a str,
    /// Float bits of d, drev, spfL, spfR, mapping cost; `None` for "-".
    bits: [Option<u32>; 5],
    map: &'a str,
    apm: Option<u32>,
}

const FIELDS: [&str; 5] = ["d", "drev", "spfL", "spfR", "mc"];

fn parse_bits(s: &str) -> Option<u32> {
    (s != "-").then(|| u32::from_str_radix(s, 16).unwrap())
}

fn parse_golden(text: &str) -> Vec<Golden<'_>> {
    text.lines()
        .enumerate()
        .map(|(i, l)| {
            let f: Vec<&str> = l.split('\t').collect();
            assert_eq!(f.len(), 11, "line {}: expected 11 columns", i + 1);
            Golden {
                line: i + 1,
                kind: f[0],
                t1: f[1],
                t2: f[2],
                model: f[3],
                bits: [f[4], f[5], f[6], f[7], f[8]].map(parse_bits),
                map: f[9],
                apm: parse_bits(f[10]),
            }
        })
        .collect()
}

/// What this crate computes for one case, in the golden file's terms.
struct Got {
    bits: [Option<u32>; 5],
    map: String,
    apm: Option<u32>,
    zhang_shasha: f32,
    zhang_shasha_rev: f32,
}

fn compute<C: CostModel<StringNodeData> + Copy>(cm: C, g: &Golden<'_>, need_oracle: bool) -> Got {
    let p = BracketStringInputParser::new();
    let (t1, t2) = (p.from_string(g.t1), p.from_string(g.t2));
    let mut a = APTED::new(cm);
    let d = a.compute_edit_distance(&t1, &t2);
    let mapping = a.compute_edit_mapping();
    let mc = a.mapping_cost(&mapping);
    let drev = APTED::new(cm).compute_edit_distance(&t2, &t1);
    // Golden "-" means Java skipped the forced-path runs (large trees).
    let forced = |path| {
        g.bits[2 + path as usize].map(|_| {
            APTED::new(cm)
                .compute_edit_distance_spf_test(&t1, &t2, path)
                .to_bits()
        })
    };
    let map = mapping
        .iter()
        .map(|p| format!("{}:{}", p[0], p[1]))
        .collect::<Vec<_>>()
        .join(",");
    let apm = g.apm.map(|_| {
        AllPossibleMappingsTED::new(cm)
            .compute_edit_distance(&t1, &t2)
            .to_bits()
    });
    let (zs, zs_rev) = if need_oracle {
        (
            oracle::zhang_shasha(&cm, &t1, &t2),
            oracle::zhang_shasha(&cm, &t2, &t1),
        )
    } else {
        (0.0, 0.0)
    };
    Got {
        bits: [
            Some(d.to_bits()),
            Some(drev.to_bits()),
            forced(0),
            forced(1),
            Some(mc.to_bits()),
        ],
        map,
        apm,
        zhang_shasha: zs,
        zhang_shasha_rev: zs_rev,
    }
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-3 * (1.0 + a.abs().max(b.abs()))
}

#[test]
fn matches_java_output_bit_for_bit_except_documented_differences() {
    let text = resource("cases.tsv");
    let golden = parse_golden(&text);
    assert!(
        golden.len() > 3000,
        "golden file truncated: {} cases",
        golden.len()
    );

    let div_text = resource("spf1_divergences.tsv");
    let expected: BTreeSet<(usize, String)> = div_text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f[0].parse().unwrap(), f[1].to_string())
        })
        .collect();
    let mut seen: BTreeSet<(usize, String)> = BTreeSet::new();
    let mut failures: Vec<String> = Vec::new();

    for g in &golden {
        let listed = |field: &str| expected.contains(&(g.line, field.to_string()));
        let need_oracle = FIELDS.iter().any(|f| listed(f));
        let got = if g.model == "unit" {
            compute(StringUnitCostModel, g, need_oracle)
        } else {
            let v: Vec<f32> = g.model.split(',').map(|x| x.parse().unwrap()).collect();
            compute(
                PerEditOperationStringNodeDataCostModel::new(v[0], v[1], v[2]),
                g,
                need_oracle,
            )
        };
        let label = format!("line {} ({} {} {} {})", g.line, g.kind, g.t1, g.t2, g.model);

        for (n, field) in FIELDS.iter().enumerate() {
            let (java, rust) = (g.bits[n], got.bits[n]);
            if java == rust {
                if listed(field) {
                    failures.push(format!(
                        "{label}: {field} is listed as divergent but matches"
                    ));
                }
                continue;
            }
            if !listed(field) {
                failures.push(format!(
                    "{label}: {field} java {:?} rust {:?}",
                    java.map(f32::from_bits),
                    rust.map(f32::from_bits)
                ));
                continue;
            }
            seen.insert((g.line, field.to_string()));
            let (java, rust) = (f32::from_bits(java.unwrap()), f32::from_bits(rust.unwrap()));
            let truth = if *field == "drev" {
                got.zhang_shasha_rev
            } else {
                got.zhang_shasha
            };
            if !(rust > java && close(rust, truth) && !close(java, truth)) {
                failures.push(format!(
                    "{label}: {field} java {java} rust {rust} zhang-shasha {truth}: \
                     expected Java to undercount and Rust to equal Zhang-Shasha"
                ));
            }
        }
        if got.map != g.map {
            failures.push(format!(
                "{label}: mapping pairs differ\n  java {}\n  rust {}",
                g.map, got.map
            ));
        }
        if let (Some(java), Some(rust)) = (g.apm, got.apm) {
            let cap = (p_nodes(g.t1) + p_nodes(g.t2)) as f32;
            let expected_java = f32::from_bits(rust).min(cap).to_bits();
            if java != expected_java {
                failures.push(format!(
                    "{label}: brute force java {} rust {} (cap {cap})",
                    f32::from_bits(java),
                    f32::from_bits(rust)
                ));
            }
        }
    }
    for cell in expected.difference(&seen) {
        failures.push(format!("listed divergence {cell:?} never observed"));
    }
    assert!(
        failures.is_empty(),
        "{} mismatches against the Java golden file, first 10:\n{}",
        failures.len(),
        failures
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

fn p_nodes(s: &str) -> usize {
    BracketStringInputParser::new().from_string(s).node_count()
}

/// The golden file must keep covering what it was built for.
#[test]
fn golden_file_covers_every_kind_and_the_spf1_cells() {
    let text = resource("cases.tsv");
    let golden = parse_golden(&text);
    for kind in ["fixture", "small", "single", "large", "shape"] {
        let n = golden.iter().filter(|g| g.kind.starts_with(kind)).count();
        assert!(n > 0, "no {kind} cases in the golden file");
    }
    let models: BTreeSet<&str> = golden.iter().map(|g| g.model).collect();
    assert!(models.contains("unit") && models.contains("1.3,0.17,0.91"));
    assert!(golden.iter().any(|g| g.apm.is_some()));
    let div_text = resource("spf1_divergences.tsv");
    let cells = div_text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .count();
    assert!(cells > 0, "the spf1 divergence list is empty");
}
