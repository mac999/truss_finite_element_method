//! Minimal dense linear algebra used by the FEM solver.
//!
//! The original C++ engine left matrix inversion and the linear solve as empty
//! stubs (`FemMetrix::Inverse`, `FemEngine::Solve`). Here we implement a robust
//! Gaussian elimination with partial pivoting, which is all the direct-stiffness
//! method needs to solve `K d = F` for the free degrees of freedom.

/// A simple row-major dense square/rectangular matrix of `f64`.
#[derive(Clone, Debug)]
pub struct Matrix {
    pub rows: usize,
    pub cols: usize,
    data: Vec<f64>,
}

impl Matrix {
    /// Allocate a `rows x cols` matrix filled with zeros.
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Matrix {
            rows,
            cols,
            data: vec![0.0; rows * cols],
        }
    }

    #[inline]
    pub fn get(&self, i: usize, j: usize) -> f64 {
        self.data[i * self.cols + j]
    }

    #[inline]
    pub fn set(&mut self, i: usize, j: usize, v: f64) {
        self.data[i * self.cols + j] = v;
    }

    #[inline]
    pub fn add(&mut self, i: usize, j: usize, v: f64) {
        self.data[i * self.cols + j] += v;
    }

    /// Multiply this matrix by a column vector.
    pub fn mul_vec(&self, x: &[f64]) -> Vec<f64> {
        assert_eq!(self.cols, x.len(), "dimension mismatch in mul_vec");
        let mut out = vec![0.0; self.rows];
        for i in 0..self.rows {
            let mut acc = 0.0;
            for j in 0..self.cols {
                acc += self.get(i, j) * x[j];
            }
            out[i] = acc;
        }
        out
    }
}

/// Solve the dense linear system `a x = b` in place using Gaussian elimination
/// with partial pivoting.
///
/// Returns `None` when the matrix is singular (e.g. an under-constrained truss
/// that still has a rigid-body mechanism after applying supports).
pub fn solve_linear_system(a: &Matrix, b: &[f64]) -> Option<Vec<f64>> {
    let n = a.rows;
    assert_eq!(a.rows, a.cols, "solve requires a square matrix");
    assert_eq!(b.len(), n, "right-hand side size mismatch");

    // Build an augmented working copy so the caller's matrix is untouched.
    let mut m = vec![0.0f64; n * (n + 1)];
    for i in 0..n {
        for j in 0..n {
            m[i * (n + 1) + j] = a.get(i, j);
        }
        m[i * (n + 1) + n] = b[i];
    }

    let stride = n + 1;
    for col in 0..n {
        // Partial pivot: find the row with the largest magnitude in this column.
        let mut pivot_row = col;
        let mut pivot_val = m[col * stride + col].abs();
        for r in (col + 1)..n {
            let v = m[r * stride + col].abs();
            if v > pivot_val {
                pivot_val = v;
                pivot_row = r;
            }
        }
        if pivot_val < 1.0e-12 {
            return None; // singular / mechanism
        }
        if pivot_row != col {
            for k in 0..stride {
                m.swap(pivot_row * stride + k, col * stride + k);
            }
        }

        // Eliminate below the pivot.
        let pivot = m[col * stride + col];
        for r in (col + 1)..n {
            let factor = m[r * stride + col] / pivot;
            if factor == 0.0 {
                continue;
            }
            for k in col..stride {
                m[r * stride + k] -= factor * m[col * stride + k];
            }
        }
    }

    // Back-substitution.
    let mut x = vec![0.0f64; n];
    for i in (0..n).rev() {
        let mut acc = m[i * stride + n];
        for j in (i + 1)..n {
            acc -= m[i * stride + j] * x[j];
        }
        x[i] = acc / m[i * stride + i];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_small_system() {
        // 2x + y = 5 ; x + 3y = 10  ->  x = 1, y = 3
        let mut a = Matrix::zeros(2, 2);
        a.set(0, 0, 2.0);
        a.set(0, 1, 1.0);
        a.set(1, 0, 1.0);
        a.set(1, 1, 3.0);
        let x = solve_linear_system(&a, &[5.0, 10.0]).unwrap();
        assert!((x[0] - 1.0).abs() < 1e-9);
        assert!((x[1] - 3.0).abs() < 1e-9);
    }

    #[test]
    fn detects_singular() {
        let mut a = Matrix::zeros(2, 2);
        a.set(0, 0, 1.0);
        a.set(0, 1, 2.0);
        a.set(1, 0, 2.0);
        a.set(1, 1, 4.0);
        assert!(solve_linear_system(&a, &[1.0, 2.0]).is_none());
    }
}
