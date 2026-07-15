//! Truss model data structures and input-file parser.
//!
//! The input format is backward-compatible with the original `TrustInput.txt`
//! used by the legacy MFC program, with a few robustness improvements:
//!
//! ```text
//! <title>                                   # free-form title line
//! <numElements>,<numNodes>                  # counts
//! id, fixX, fixY, fixZ, x, y, z, Fx, Fy, Fz # one line per node
//! ...
//! id, nodeA, nodeB, E, A                    # one line per element
//! ...
//! ```
//!
//! Improvements over the original:
//! * Blank lines and `#` comments are ignored.
//! * Fields may be separated by commas and/or whitespace.
//! * Elements reference nodes by their *id* (mapped to an internal index),
//!   fixing the off-by-one bug in the legacy `GetVertexAt` code path.
//! * Every malformed line yields a precise, English error message.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

/// A single truss joint (node).
#[derive(Clone, Debug)]
pub struct Node {
    /// User-facing id as written in the input file.
    pub id: i64,
    /// Constraint flags per axis: `true` means the DOF is fixed (a support).
    pub fixed: [bool; 3],
    /// Coordinates `[x, y, z]`.
    pub coord: [f64; 3],
    /// Applied nodal load `[Fx, Fy, Fz]`.
    pub force: [f64; 3],
}

/// A two-node axial truss member.
#[derive(Clone, Debug)]
pub struct Element {
    pub id: i64,
    /// Internal node indices (0-based into [`Model::nodes`]).
    pub nodes: [usize; 2],
    /// Young's modulus (elastic modulus), `E`.
    pub e: f64,
    /// Cross-sectional area, `A`.
    pub area: f64,
}

/// A complete truss model ready for analysis.
#[derive(Clone, Debug)]
pub struct Model {
    pub title: String,
    pub nodes: Vec<Node>,
    pub elements: Vec<Element>,
}

/// A parse/validation error with a human-readable, English message.
#[derive(Debug)]
pub struct ModelError(pub String);

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ModelError {}

impl Element {
    /// Length of the member in the current (undeformed) geometry.
    pub fn length(&self, nodes: &[Node]) -> f64 {
        let a = &nodes[self.nodes[0]].coord;
        let b = &nodes[self.nodes[1]].coord;
        let mut sum = 0.0;
        for k in 0..3 {
            let d = b[k] - a[k];
            sum += d * d;
        }
        sum.sqrt()
    }

    /// Direction cosines `[cx, cy, cz]` from node A to node B.
    pub fn direction_cosines(&self, nodes: &[Node]) -> [f64; 3] {
        let a = &nodes[self.nodes[0]].coord;
        let b = &nodes[self.nodes[1]].coord;
        let len = self.length(nodes);
        if len < 1.0e-12 {
            return [0.0; 3];
        }
        [
            (b[0] - a[0]) / len,
            (b[1] - a[1]) / len,
            (b[2] - a[2]) / len,
        ]
    }
}

impl Model {
    /// Total number of degrees of freedom (`3 * node_count`).
    pub fn dof_count(&self) -> usize {
        self.nodes.len() * 3
    }

    /// Whether the model is effectively planar (all z-coordinates ~ 0).
    pub fn is_planar(&self) -> bool {
        self.nodes.iter().all(|n| n.coord[2].abs() < 1.0e-9)
    }

    /// Load a model from an input file on disk.
    pub fn from_file(path: &Path) -> Result<Model, ModelError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| ModelError(format!("cannot read '{}': {}", path.display(), e)))?;
        Model::from_str(&text)
    }

    /// Parse a model from the textual input format.
    pub fn from_str(text: &str) -> Result<Model, ModelError> {
        // Keep only meaningful lines, but preserve the *first* line verbatim as
        // the title even if it looks like a comment.
        let mut raw_lines = text.lines();

        let title = raw_lines
            .next()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ModelError("input is empty: expected a title line".into()))?;

        // Remaining data lines with comments/blanks stripped.
        let mut data: Vec<String> = Vec::new();
        for line in raw_lines {
            let line = strip_comment(line);
            if !line.trim().is_empty() {
                data.push(line.to_string());
            }
        }

        let mut cursor = data.into_iter();

        // Counts line: <numElements>,<numNodes>
        let counts = cursor
            .next()
            .ok_or_else(|| ModelError("missing counts line (expected '<elements>,<nodes>')".into()))?;
        let count_tokens = tokenize(&counts);
        if count_tokens.len() < 2 {
            return Err(ModelError(format!(
                "counts line must have 2 values '<elements>,<nodes>', got: '{}'",
                counts.trim()
            )));
        }
        let num_elements = parse_usize(&count_tokens[0], "element count")?;
        let num_nodes = parse_usize(&count_tokens[1], "node count")?;

        // Node lines.
        let mut nodes = Vec::with_capacity(num_nodes);
        for i in 0..num_nodes {
            let line = cursor.next().ok_or_else(|| {
                ModelError(format!(
                    "expected {} node lines but only found {}",
                    num_nodes, i
                ))
            })?;
            nodes.push(parse_node(&line)?);
        }

        // Build id -> index map, rejecting duplicates.
        let mut id_to_index: HashMap<i64, usize> = HashMap::new();
        for (idx, node) in nodes.iter().enumerate() {
            if id_to_index.insert(node.id, idx).is_some() {
                return Err(ModelError(format!("duplicate node id {}", node.id)));
            }
        }

        // Element lines.
        let mut elements = Vec::with_capacity(num_elements);
        for i in 0..num_elements {
            let line = cursor.next().ok_or_else(|| {
                ModelError(format!(
                    "expected {} element lines but only found {}",
                    num_elements, i
                ))
            })?;
            elements.push(parse_element(&line, &id_to_index)?);
        }

        let model = Model {
            title,
            nodes,
            elements,
        };
        model.validate()?;
        Ok(model)
    }

    /// Serialize the model back into the textual input format, so an
    /// in-memory model can be saved and re-loaded (`save-model` in the shell).
    pub fn to_input_string(&self) -> String {
        use std::fmt::Write as _;
        let mut s = String::new();
        let _ = writeln!(s, "{}", self.title);
        let _ = writeln!(s, "{},{}", self.elements.len(), self.nodes.len());
        let _ = writeln!(s, "# id, fixX, fixY, fixZ, x, y, z, Fx, Fy, Fz");
        for n in &self.nodes {
            let _ = writeln!(
                s,
                "{},{},{},{},{},{},{},{},{},{}",
                n.id,
                n.fixed[0] as u8, n.fixed[1] as u8, n.fixed[2] as u8,
                n.coord[0], n.coord[1], n.coord[2],
                n.force[0], n.force[1], n.force[2],
            );
        }
        let _ = writeln!(s, "# id, nodeA, nodeB, E, A");
        for e in &self.elements {
            let _ = writeln!(
                s,
                "{},{},{},{},{}",
                e.id,
                self.nodes[e.nodes[0]].id,
                self.nodes[e.nodes[1]].id,
                e.e,
                e.area,
            );
        }
        s
    }

    /// Sanity checks that catch common modelling mistakes early.
    fn validate(&self) -> Result<(), ModelError> {
        if self.nodes.is_empty() {
            return Err(ModelError("model has no nodes".into()));
        }
        if self.elements.is_empty() {
            return Err(ModelError("model has no elements".into()));
        }
        for e in &self.elements {
            if e.nodes[0] == e.nodes[1] {
                return Err(ModelError(format!(
                    "element {} connects a node to itself",
                    e.id
                )));
            }
            if e.length(&self.nodes) < 1.0e-9 {
                return Err(ModelError(format!(
                    "element {} has zero length (coincident nodes)",
                    e.id
                )));
            }
            if e.e <= 0.0 {
                return Err(ModelError(format!(
                    "element {} has non-positive elastic modulus E={}",
                    e.id, e.e
                )));
            }
            if e.area <= 0.0 {
                return Err(ModelError(format!(
                    "element {} has non-positive area A={}",
                    e.id, e.area
                )));
            }
        }
        Ok(())
    }
}

/// Remove a trailing `#` comment from a line.
fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(pos) => &line[..pos],
        None => line,
    }
}

/// Split on commas and/or ASCII whitespace, dropping empty tokens.
fn tokenize(line: &str) -> Vec<String> {
    line.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn parse_usize(s: &str, what: &str) -> Result<usize, ModelError> {
    s.trim()
        .parse::<usize>()
        .map_err(|_| ModelError(format!("invalid {}: '{}'", what, s)))
}

fn parse_i64(s: &str, what: &str) -> Result<i64, ModelError> {
    s.trim()
        .parse::<i64>()
        .map_err(|_| ModelError(format!("invalid {}: '{}'", what, s)))
}

fn parse_f64(s: &str, what: &str) -> Result<f64, ModelError> {
    s.trim()
        .parse::<f64>()
        .map_err(|_| ModelError(format!("invalid {}: '{}'", what, s)))
}

fn parse_node(line: &str) -> Result<Node, ModelError> {
    // id, fixX, fixY, fixZ, x, y, z, Fx, Fy, Fz
    let t = tokenize(line);
    if t.len() < 10 {
        return Err(ModelError(format!(
            "node line needs 10 values (id, fixX, fixY, fixZ, x, y, z, Fx, Fy, Fz), got {}: '{}'",
            t.len(),
            line.trim()
        )));
    }
    let id = parse_i64(&t[0], "node id")?;
    let fixed = [
        parse_i64(&t[1], "fixX")? != 0,
        parse_i64(&t[2], "fixY")? != 0,
        parse_i64(&t[3], "fixZ")? != 0,
    ];
    let coord = [
        parse_f64(&t[4], "x")?,
        parse_f64(&t[5], "y")?,
        parse_f64(&t[6], "z")?,
    ];
    let force = [
        parse_f64(&t[7], "Fx")?,
        parse_f64(&t[8], "Fy")?,
        parse_f64(&t[9], "Fz")?,
    ];
    Ok(Node {
        id,
        fixed,
        coord,
        force,
    })
}

fn parse_element(line: &str, id_to_index: &HashMap<i64, usize>) -> Result<Element, ModelError> {
    // id, nodeA, nodeB, E, A
    let t = tokenize(line);
    if t.len() < 5 {
        return Err(ModelError(format!(
            "element line needs 5 values (id, nodeA, nodeB, E, A), got {}: '{}'",
            t.len(),
            line.trim()
        )));
    }
    let id = parse_i64(&t[0], "element id")?;
    let node_a_id = parse_i64(&t[1], "element nodeA")?;
    let node_b_id = parse_i64(&t[2], "element nodeB")?;
    let e = parse_f64(&t[3], "elastic modulus E")?;
    let area = parse_f64(&t[4], "area A")?;

    let resolve = |nid: i64| -> Result<usize, ModelError> {
        id_to_index
            .get(&nid)
            .copied()
            .ok_or_else(|| ModelError(format!("element {} references unknown node id {}", id, nid)))
    };

    Ok(Element {
        id,
        nodes: [resolve(node_a_id)?, resolve(node_b_id)?],
        e,
        area,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "SPACE TRUSS EXAMPLE OF SECTION\n\
        3,4\n\
        1,0,1,0,72.0,0.,0.,0.,0.,-1000.0\n\
        2,1,1,1,0.0,36.0,0.,0.,0.,0.\n\
        3,1,1,1,0.0,36.0,72.0,0.,0.,0.\n\
        4,1,1,1,0.0,0.0,-48.0,0.,0.,0.\n\
        1,1,4,1.2E+6,0.187\n\
        2,1,2,1.2E+6,0.302\n\
        3,1,3,1.2E+6,0.726\n";

    #[test]
    fn parses_sample() {
        let m = Model::from_str(SAMPLE).unwrap();
        assert_eq!(m.nodes.len(), 4);
        assert_eq!(m.elements.len(), 3);
        // Element 1 connects node id 1 (index 0) and node id 4 (index 3).
        assert_eq!(m.elements[0].nodes, [0, 3]);
        assert_eq!(m.nodes[0].force[2], -1000.0);
        assert!(m.nodes[1].fixed.iter().all(|&f| f));
    }

    #[test]
    fn input_format_round_trips() {
        let m = Model::from_str(SAMPLE).unwrap();
        let m2 = Model::from_str(&m.to_input_string()).unwrap();
        assert_eq!(m2.nodes.len(), m.nodes.len());
        assert_eq!(m2.elements.len(), m.elements.len());
        assert_eq!(m2.elements[0].nodes, m.elements[0].nodes);
        assert_eq!(m2.nodes[0].force, m.nodes[0].force);
        assert_eq!(m2.title, m.title);
    }
}
