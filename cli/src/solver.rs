//! Direct-stiffness finite element solver for space/plane trusses.
//!
//! This is the piece the legacy C++ program never finished: `FemEngine::Action`,
//! `Solve`, `GetForce`, and the matrix routines were all empty stubs. Here the
//! full pipeline is implemented:
//!
//! 1. Assemble the global stiffness matrix `K` from every element's local
//!    stiffness `(EA/L) * [[B, -B], [-B, B]]`, where `B = c cᵀ` is the outer
//!    product of the element's direction-cosine vector.
//! 2. Partition DOFs into free / fixed sets from the support flags.
//! 3. Solve `K_ff d_f = F_f` for the free displacements.
//! 4. Recover support reactions `R = K d - F`.
//! 5. Compute each member's axial force, stress, strain and elongation.

use crate::linalg::{solve_linear_system, Matrix};
use crate::model::Model;

/// Per-element analysis result.
#[derive(Clone, Debug)]
pub struct ElementResult {
    pub id: i64,
    pub node_ids: [i64; 2],
    pub length: f64,
    /// Axial force; positive = tension, negative = compression.
    pub axial_force: f64,
    pub stress: f64,
    pub strain: f64,
    /// Change in length (positive = elongation).
    pub elongation: f64,
}

impl ElementResult {
    pub fn is_tension(&self) -> bool {
        self.axial_force > 1.0e-9
    }
    pub fn is_compression(&self) -> bool {
        self.axial_force < -1.0e-9
    }
    pub fn state_label(&self) -> &'static str {
        if self.is_tension() {
            "tension"
        } else if self.is_compression() {
            "compression"
        } else {
            "zero-force"
        }
    }
}

/// Per-node analysis result.
#[derive(Clone, Debug)]
pub struct NodeResult {
    pub id: i64,
    pub displacement: [f64; 3],
    /// Reaction force at supported DOFs (zero at free DOFs).
    pub reaction: [f64; 3],
    /// Whether each DOF is a support (mirrors the input fix flags).
    pub fixed: [bool; 3],
}

/// Complete analysis output.
#[derive(Clone, Debug)]
pub struct Solution {
    pub title: String,
    pub planar: bool,
    pub nodes: Vec<NodeResult>,
    pub elements: Vec<ElementResult>,
    /// Largest absolute nodal displacement component (useful for scaling views).
    pub max_abs_displacement: f64,
}

/// Reasons the analysis cannot proceed.
#[derive(Debug)]
pub enum SolveError {
    /// Every DOF is fixed — nothing to solve.
    NoFreeDof,
    /// The reduced stiffness matrix is singular: the structure is a mechanism
    /// (under-constrained / unstable) for the given supports.
    Singular,
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SolveError::NoFreeDof => write!(
                f,
                "all degrees of freedom are fixed: the model has nothing to solve"
            ),
            SolveError::Singular => write!(
                f,
                "the stiffness matrix is singular: the truss is unstable (a mechanism) \
                 for the given supports — add constraints or members"
            ),
        }
    }
}

impl std::error::Error for SolveError {}

/// Run the full direct-stiffness analysis on a model.
pub fn solve(model: &Model) -> Result<Solution, SolveError> {
    let ndof = model.dof_count();

    // --- 1. Assemble the global stiffness matrix ------------------------------
    let mut k = Matrix::zeros(ndof, ndof);
    for el in &model.elements {
        let len = el.length(&model.nodes);
        let c = el.direction_cosines(&model.nodes);
        let ea_over_l = el.e * el.area / len;

        // 3x3 outer-product block B = c cᵀ.
        let mut b = [[0.0f64; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                b[i][j] = c[i] * c[j];
            }
        }

        let ia = el.nodes[0] * 3;
        let ib = el.nodes[1] * 3;
        // Scatter the 6x6 element matrix into the global matrix.
        for i in 0..3 {
            for j in 0..3 {
                let v = ea_over_l * b[i][j];
                k.add(ia + i, ia + j, v); // top-left  (+B)
                k.add(ib + i, ib + j, v); // bottom-right (+B)
                k.add(ia + i, ib + j, -v); // top-right (-B)
                k.add(ib + i, ia + j, -v); // bottom-left (-B)
            }
        }
    }

    // --- 2. Partition DOFs into free / fixed ---------------------------------
    let mut is_fixed = vec![false; ndof];
    for (n, node) in model.nodes.iter().enumerate() {
        for axis in 0..3 {
            if node.fixed[axis] {
                is_fixed[n * 3 + axis] = true;
            }
        }
    }
    let free_dofs: Vec<usize> = (0..ndof).filter(|&d| !is_fixed[d]).collect();
    if free_dofs.is_empty() {
        return Err(SolveError::NoFreeDof);
    }

    // Applied load vector.
    let mut f = vec![0.0f64; ndof];
    for (n, node) in model.nodes.iter().enumerate() {
        for axis in 0..3 {
            f[n * 3 + axis] = node.force[axis];
        }
    }

    // --- 3. Build and solve the reduced system K_ff d_f = F_f -----------------
    let nf = free_dofs.len();
    let mut kff = Matrix::zeros(nf, nf);
    let mut ff = vec![0.0f64; nf];
    for (ri, &gr) in free_dofs.iter().enumerate() {
        ff[ri] = f[gr];
        for (ci, &gc) in free_dofs.iter().enumerate() {
            kff.set(ri, ci, k.get(gr, gc));
        }
    }
    let df = solve_linear_system(&kff, &ff).ok_or(SolveError::Singular)?;

    // Scatter free displacements back into the full displacement vector.
    let mut d = vec![0.0f64; ndof];
    for (ri, &gr) in free_dofs.iter().enumerate() {
        d[gr] = df[ri];
    }

    // --- 4. Recover reactions R = K d - F ------------------------------------
    let kd = k.mul_vec(&d);
    let mut reaction = vec![0.0f64; ndof];
    for dof in 0..ndof {
        if is_fixed[dof] {
            reaction[dof] = kd[dof] - f[dof];
        }
    }

    // --- 5. Assemble per-node results ----------------------------------------
    let mut max_abs = 0.0f64;
    let mut node_results = Vec::with_capacity(model.nodes.len());
    for (n, node) in model.nodes.iter().enumerate() {
        let disp = [d[n * 3], d[n * 3 + 1], d[n * 3 + 2]];
        for &v in &disp {
            max_abs = max_abs.max(v.abs());
        }
        node_results.push(NodeResult {
            id: node.id,
            displacement: disp,
            reaction: [
                reaction[n * 3],
                reaction[n * 3 + 1],
                reaction[n * 3 + 2],
            ],
            fixed: node.fixed,
        });
    }

    // --- 6. Element forces ----------------------------------------------------
    // Axial force N = (EA/L) * c · (d_j - d_i); positive means tension.
    let mut element_results = Vec::with_capacity(model.elements.len());
    for el in &model.elements {
        let len = el.length(&model.nodes);
        let c = el.direction_cosines(&model.nodes);
        let ia = el.nodes[0] * 3;
        let ib = el.nodes[1] * 3;

        let mut axial_disp = 0.0; // c · (d_j - d_i) == elongation
        for axis in 0..3 {
            axial_disp += c[axis] * (d[ib + axis] - d[ia + axis]);
        }
        let strain = axial_disp / len;
        let stress = el.e * strain;
        let axial_force = stress * el.area;

        element_results.push(ElementResult {
            id: el.id,
            node_ids: [
                model.nodes[el.nodes[0]].id,
                model.nodes[el.nodes[1]].id,
            ],
            length: len,
            axial_force,
            stress,
            strain,
            elongation: axial_disp,
        });
    }

    Ok(Solution {
        title: model.title.clone(),
        planar: model.is_planar(),
        nodes: node_results,
        elements: element_results,
        max_abs_displacement: max_abs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Model;

    // Classic textbook space-truss example. The single free node (id 1) is
    // constrained only in y and loaded with -1000 in z. Known result: the
    // vertical displacement is negative (node moves down) and the members carry
    // a mix of tension/compression that equilibrates the applied load.
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
    fn equilibrium_is_satisfied() {
        let model = Model::from_str(SAMPLE).unwrap();
        let sol = solve(&model).unwrap();

        // Sum of all reactions + applied loads must be ~zero (global equilibrium).
        let mut sum = [0.0f64; 3];
        for nr in &sol.nodes {
            for axis in 0..3 {
                sum[axis] += nr.reaction[axis];
            }
        }
        // Applied load is -1000 in z at node 1.
        sum[2] += -1000.0;
        for axis in 0..3 {
            assert!(sum[axis].abs() < 1e-3, "axis {} not balanced: {}", axis, sum[axis]);
        }
    }

    #[test]
    fn loaded_node_moves_down() {
        let model = Model::from_str(SAMPLE).unwrap();
        let sol = solve(&model).unwrap();
        let n1 = sol.nodes.iter().find(|n| n.id == 1).unwrap();
        assert!(n1.displacement[2] < 0.0, "node should deflect downward");
    }
}
