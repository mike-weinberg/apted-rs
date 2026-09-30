//! Differential driver: reads "id<TAB>kind<TAB>t1<TAB>t2<TAB>model" lines on
//! stdin and prints the same line format as `GoldenDump.java`.

use apted::*;
use std::io::{BufRead, Write};

fn run<C: CostModel<StringNodeData> + Copy>(
    cm: C,
    t1: &Node<StringNodeData>,
    t2: &Node<StringNodeData>,
    big: bool,
    small: bool,
    out: &mut String,
) {
    use std::fmt::Write;
    let mut a = APTED::new(cm);
    let d = a.compute_edit_distance(t1, t2);
    let map = a.compute_edit_mapping();
    let mc = a.mapping_cost(&map);
    let drev = APTED::new(cm).compute_edit_distance(t2, t1);
    let (sl, sr);
    if big {
        sl = 0;
        sr = 0;
    } else {
        sl = APTED::new(cm)
            .compute_edit_distance_spf_test(t1, t2, 0)
            .to_bits();
        sr = APTED::new(cm)
            .compute_edit_distance_spf_test(t1, t2, 1)
            .to_bits();
    }
    write!(
        out,
        "d={:08x} drev={:08x} spfL={} spfR={} mc={:08x} map=",
        d.to_bits(),
        drev.to_bits(),
        if big {
            "-".to_string()
        } else {
            format!("{:08x}", sl)
        },
        if big {
            "-".to_string()
        } else {
            format!("{:08x}", sr)
        },
        mc.to_bits()
    )
    .unwrap();
    for (i, p) in map.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write!(out, "{}:{}", p[0], p[1]).unwrap();
    }
    if small {
        let apm = AllPossibleMappingsTED::new(cm).compute_edit_distance(t1, t2);
        write!(out, " apm={:08x}", apm.to_bits()).unwrap();
    }
}

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut w = std::io::BufWriter::new(stdout.lock());
    let p = BracketStringInputParser::new();
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let f: Vec<&str> = line.split('\t').collect();
        let t1 = p.from_string(f[2]);
        let t2 = p.from_string(f[3]);
        let big = t1.node_count() > 300 || t2.node_count() > 300;
        let small = t1.node_count() <= 6 && t2.node_count() <= 6;
        let mut out = String::new();
        if f[4] == "unit" {
            run(StringUnitCostModel, &t1, &t2, big, small, &mut out);
        } else {
            let v: Vec<f32> = f[4].split(',').map(|x| x.parse().unwrap()).collect();
            run(
                PerEditOperationStringNodeDataCostModel::new(v[0], v[1], v[2]),
                &t1,
                &t2,
                big,
                small,
                &mut out,
            );
        }
        writeln!(w, "{} {}", f[0], out).unwrap();
    }
}
