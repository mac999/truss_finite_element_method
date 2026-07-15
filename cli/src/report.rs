//! Human-readable and machine-readable renderings of a [`Solution`].

use crate::model::Model;
use crate::solver::Solution;
use std::fmt::Write as _;

/// Render a clean, aligned plain-text report.
pub fn text_report(model: &Model, sol: &Solution) -> String {
    let mut s = String::new();
    let dim = if sol.planar { "2D (planar)" } else { "3D (space)" };

    let _ = writeln!(s, "============================================================");
    let _ = writeln!(s, " Truss FEM Analysis Report");
    let _ = writeln!(s, "============================================================");
    let _ = writeln!(s, " Title      : {}", sol.title);
    let _ = writeln!(s, " Type       : {} truss", dim);
    let _ = writeln!(s, " Nodes      : {}", model.nodes.len());
    let _ = writeln!(s, " Elements   : {}", model.elements.len());
    let _ = writeln!(s, " Max |disp| : {:.6e}", sol.max_abs_displacement);
    let _ = writeln!(s);

    // Nodal displacements.
    let _ = writeln!(s, "-- Nodal Displacements ------------------------------------");
    let _ = writeln!(
        s,
        "  {:>5}  {:>14}  {:>14}  {:>14}",
        "Node", "Ux", "Uy", "Uz"
    );
    for n in &sol.nodes {
        let _ = writeln!(
            s,
            "  {:>5}  {:>14.6e}  {:>14.6e}  {:>14.6e}",
            n.id, n.displacement[0], n.displacement[1], n.displacement[2]
        );
    }
    let _ = writeln!(s);

    // Support reactions.
    let _ = writeln!(s, "-- Support Reactions --------------------------------------");
    let _ = writeln!(
        s,
        "  {:>5}  {:>14}  {:>14}  {:>14}",
        "Node", "Rx", "Ry", "Rz"
    );
    for n in &sol.nodes {
        if n.fixed.iter().any(|&f| f) {
            let _ = writeln!(
                s,
                "  {:>5}  {:>14.4}  {:>14.4}  {:>14.4}",
                n.id, n.reaction[0], n.reaction[1], n.reaction[2]
            );
        }
    }
    let _ = writeln!(s);

    // Element forces.
    let _ = writeln!(s, "-- Element Forces -----------------------------------------");
    let _ = writeln!(
        s,
        "  {:>5}  {:>5} {:>5}  {:>12}  {:>14}  {:>14}  {:>12}",
        "Elem", "A", "B", "Length", "Axial Force", "Stress", "State"
    );
    for e in &sol.elements {
        let _ = writeln!(
            s,
            "  {:>5}  {:>5} {:>5}  {:>12.4}  {:>14.4}  {:>14.4}  {:>12}",
            e.id,
            e.node_ids[0],
            e.node_ids[1],
            e.length,
            e.axial_force,
            e.stress,
            e.state_label()
        );
    }
    let _ = writeln!(s);
    let _ = writeln!(s, " Sign convention: axial force/stress > 0 = tension, < 0 = compression.");
    let _ = writeln!(s, "============================================================");
    s
}

/// Render the solution (plus the input model) as pretty-printed JSON.
///
/// Hand-written to avoid a serde dependency; the schema is documented in the
/// README and consumed by the web viewer.
pub fn json_report(model: &Model, sol: &Solution) -> String {
    let mut s = String::new();
    s.push_str("{\n");
    let _ = write!(s, "  \"title\": {},\n", json_str(&sol.title));
    let _ = write!(s, "  \"planar\": {},\n", sol.planar);
    let _ = write!(s, "  \"maxAbsDisplacement\": {},\n", json_num(sol.max_abs_displacement));

    // Input geometry (so a single JSON file fully describes the analysed model).
    s.push_str("  \"nodes\": [\n");
    for (i, (m, r)) in model.nodes.iter().zip(sol.nodes.iter()).enumerate() {
        let comma = if i + 1 < model.nodes.len() { "," } else { "" };
        let _ = write!(
            s,
            "    {{ \"id\": {}, \"coord\": [{}, {}, {}], \"fixed\": [{}, {}, {}], \
             \"load\": [{}, {}, {}], \"displacement\": [{}, {}, {}], \
             \"reaction\": [{}, {}, {}] }}{}\n",
            m.id,
            json_num(m.coord[0]), json_num(m.coord[1]), json_num(m.coord[2]),
            m.fixed[0], m.fixed[1], m.fixed[2],
            json_num(m.force[0]), json_num(m.force[1]), json_num(m.force[2]),
            json_num(r.displacement[0]), json_num(r.displacement[1]), json_num(r.displacement[2]),
            json_num(r.reaction[0]), json_num(r.reaction[1]), json_num(r.reaction[2]),
            comma
        );
    }
    s.push_str("  ],\n");

    s.push_str("  \"elements\": [\n");
    for (i, e) in sol.elements.iter().enumerate() {
        let comma = if i + 1 < sol.elements.len() { "," } else { "" };
        let _ = write!(
            s,
            "    {{ \"id\": {}, \"nodes\": [{}, {}], \"length\": {}, \
             \"axialForce\": {}, \"stress\": {}, \"strain\": {}, \
             \"elongation\": {}, \"state\": {} }}{}\n",
            e.id,
            e.node_ids[0], e.node_ids[1],
            json_num(e.length),
            json_num(e.axial_force),
            json_num(e.stress),
            json_num(e.strain),
            json_num(e.elongation),
            json_str(e.state_label()),
            comma
        );
    }
    s.push_str("  ]\n");
    s.push_str("}\n");
    s
}

/// Encode a finite `f64` as JSON; non-finite values become `null`.
fn json_num(v: f64) -> String {
    if v.is_finite() {
        // Compact but lossless-ish representation.
        format!("{}", v)
    } else {
        "null".to_string()
    }
}

/// Encode a string as a JSON string literal with the necessary escapes.
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
