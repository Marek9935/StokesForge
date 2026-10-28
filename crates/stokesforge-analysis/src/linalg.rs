//! Minimal dense algebra: solving, inverse, linear least squares.
//!
//! Written here rather than borrowed from an algebra crate: the systems are
//! small (a few parameters), and staying dependency-free keeps the crate out
//! of `cargo deny` and of the notice list. Numerical stability for a
//! high-degree polynomial comes from the Householder QR decomposition, not
//! from the normal equations. QR alone does not repair an X far from 0 (the
//! columns 1, x, x², … become almost collinear): the polynomial fit
//! centers X before the QR and re-expresses the coefficients in the original
//! axis (#283, `CompiledModel::x_shift`).

/// Dense matrix stored by rows.
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<f64>,
}

impl Matrix {
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            data: vec![0.0; rows * cols],
        }
    }

    pub fn identity(n: usize) -> Self {
        let mut m = Self::zeros(n, n);
        for i in 0..n {
            m.set(i, i, 1.0);
        }
        m
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn get(&self, r: usize, c: usize) -> f64 {
        self.data[r * self.cols + c]
    }

    pub fn set(&mut self, r: usize, c: usize, v: f64) {
        self.data[r * self.cols + c] = v;
    }

    pub fn add(&mut self, r: usize, c: usize, v: f64) {
        self.data[r * self.cols + c] += v;
    }

    /// `selfᵀ · self`.
    pub fn gram(&self) -> Matrix {
        let mut g = Matrix::zeros(self.cols, self.cols);
        for r in 0..self.rows {
            let row = &self.data[r * self.cols..(r + 1) * self.cols];
            for i in 0..self.cols {
                let ri = row[i];
                if ri == 0.0 {
                    continue;
                }
                for j in i..self.cols {
                    g.data[i * self.cols + j] += ri * row[j];
                }
            }
        }
        for i in 0..self.cols {
            for j in 0..i {
                g.data[i * self.cols + j] = g.data[j * self.cols + i];
            }
        }
        g
    }

    /// `self · v`.
    pub fn mul_vec(&self, v: &[f64]) -> Vec<f64> {
        (0..self.rows)
            .map(|i| (0..self.cols).map(|j| self.get(i, j) * v[j]).sum())
            .collect()
    }

    /// `self · other`.
    pub fn mul(&self, other: &Matrix) -> Matrix {
        let mut m = Matrix::zeros(self.rows, other.cols);
        for i in 0..self.rows {
            for j in 0..other.cols {
                let s = (0..self.cols)
                    .map(|k| self.get(i, k) * other.get(k, j))
                    .sum();
                m.set(i, j, s);
            }
        }
        m
    }

    /// `selfᵀ`.
    pub fn transpose(&self) -> Matrix {
        let mut m = Matrix::zeros(self.cols, self.rows);
        for i in 0..self.rows {
            for j in 0..self.cols {
                m.set(j, i, self.get(i, j));
            }
        }
        m
    }

    /// `selfᵀ · v`.
    pub fn transpose_mul(&self, v: &[f64]) -> Vec<f64> {
        let mut out = vec![0.0; self.cols];
        for (r, &vr) in v.iter().enumerate().take(self.rows) {
            let row = &self.data[r * self.cols..(r + 1) * self.cols];
            for (o, &a) in out.iter_mut().zip(row) {
                *o += a * vr;
            }
        }
        out
    }
}

/// Solves `a · x = b` by Gaussian elimination with partial pivoting. `None` if
/// the matrix is singular (zero or non-finite pivot).
pub fn solve(a: &Matrix, b: &[f64]) -> Option<Vec<f64>> {
    let n = a.rows;
    if a.cols != n || b.len() != n {
        return None;
    }
    let mut m = a.clone();
    let mut x = b.to_vec();
    for col in 0..n {
        let mut piv = col;
        for r in col + 1..n {
            if m.get(r, col).abs() > m.get(piv, col).abs() {
                piv = r;
            }
        }
        let p = m.get(piv, col);
        if !p.is_finite() || p.abs() < 1e-300 {
            return None;
        }
        if piv != col {
            for c in 0..n {
                let t = m.get(col, c);
                m.set(col, c, m.get(piv, c));
                m.set(piv, c, t);
            }
            x.swap(col, piv);
        }
        for r in col + 1..n {
            let f = m.get(r, col) / m.get(col, col);
            if f == 0.0 {
                continue;
            }
            for c in col..n {
                let v = m.get(col, c);
                m.add(r, c, -f * v);
            }
            x[r] -= f * x[col];
        }
    }
    for i in (0..n).rev() {
        let mut s = x[i];
        for j in i + 1..n {
            s -= m.get(i, j) * x[j];
        }
        x[i] = s / m.get(i, i);
        if !x[i].is_finite() {
            return None;
        }
    }
    Some(x)
}

/// Inverse of a square matrix (Gauss-Jordan with partial pivoting).
pub fn invert(a: &Matrix) -> Option<Matrix> {
    let n = a.rows;
    if a.cols != n {
        return None;
    }
    let mut m = a.clone();
    let mut inv = Matrix::identity(n);
    for col in 0..n {
        let mut piv = col;
        for r in col + 1..n {
            if m.get(r, col).abs() > m.get(piv, col).abs() {
                piv = r;
            }
        }
        let p = m.get(piv, col);
        if !p.is_finite() || p.abs() < 1e-300 {
            return None;
        }
        if piv != col {
            for c in 0..n {
                let t = m.get(col, c);
                m.set(col, c, m.get(piv, c));
                m.set(piv, c, t);
                let t = inv.get(col, c);
                inv.set(col, c, inv.get(piv, c));
                inv.set(piv, c, t);
            }
        }
        let d = m.get(col, col);
        for c in 0..n {
            m.set(col, c, m.get(col, c) / d);
            inv.set(col, c, inv.get(col, c) / d);
        }
        for r in 0..n {
            if r == col {
                continue;
            }
            let f = m.get(r, col);
            if f == 0.0 {
                continue;
            }
            for c in 0..n {
                let (mv, iv) = (m.get(col, c), inv.get(col, c));
                m.add(r, c, -f * mv);
                inv.add(r, c, -f * iv);
            }
        }
    }
    Some(inv)
}

/// Result of a linear least-squares fit.
#[derive(Debug, Clone, PartialEq)]
pub struct LeastSquares {
    pub coefficients: Vec<f64>,
    /// `(AᵀA)⁻¹`, without scaling by the residual variance.
    pub unscaled_covariance: Matrix,
}

/// Least squares `min ‖A·c − y‖²` by Householder QR. `a` has more rows
/// than columns; `None` if the rank is insufficient.
pub fn least_squares(a: &Matrix, y: &[f64]) -> Option<LeastSquares> {
    let (m, n) = (a.rows, a.cols);
    if m < n || n == 0 || y.len() != m {
        return None;
    }
    let mut r = a.clone();
    let mut qty = y.to_vec();
    for k in 0..n {
        let mut norm = 0.0;
        for i in k..m {
            norm += r.get(i, k) * r.get(i, k);
        }
        let norm = norm.sqrt();
        if !norm.is_finite() || norm < 1e-300 {
            return None;
        }
        let alpha = if r.get(k, k) > 0.0 { -norm } else { norm };
        let mut v: Vec<f64> = (k..m).map(|i| r.get(i, k)).collect();
        v[0] -= alpha;
        let vnorm2: f64 = v.iter().map(|t| t * t).sum();
        if vnorm2 < 1e-300 {
            continue;
        }
        for j in k..n {
            let dot: f64 = (k..m).map(|i| v[i - k] * r.get(i, j)).sum();
            let f = 2.0 * dot / vnorm2;
            for i in k..m {
                r.add(i, j, -f * v[i - k]);
            }
        }
        let dot: f64 = (k..m).map(|i| v[i - k] * qty[i]).sum();
        let f = 2.0 * dot / vnorm2;
        for i in k..m {
            qty[i] -= f * v[i - k];
        }
    }
    let mut rn = Matrix::zeros(n, n);
    let mut scale = 0.0_f64;
    for i in 0..n {
        for j in i..n {
            rn.set(i, j, r.get(i, j));
        }
        scale = scale.max(r.get(i, i).abs());
    }
    for i in 0..n {
        if !(rn.get(i, i).abs() > 1e-13 * scale) {
            return None;
        }
    }
    let mut c = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = qty[i];
        for j in i + 1..n {
            s -= rn.get(i, j) * c[j];
        }
        c[i] = s / rn.get(i, i);
    }
    // (AᵀA)⁻¹ = R⁻¹ R⁻ᵀ.
    let mut rinv = Matrix::zeros(n, n);
    for j in 0..n {
        for i in (0..=j).rev() {
            let mut s = if i == j { 1.0 } else { 0.0 };
            for k in i + 1..=j {
                s -= rn.get(i, k) * rinv.get(k, j);
            }
            rinv.set(i, j, s / rn.get(i, i));
        }
    }
    let mut cov = Matrix::zeros(n, n);
    for i in 0..n {
        for j in 0..n {
            let mut s = 0.0;
            for k in i.max(j)..n {
                s += rinv.get(i, k) * rinv.get(j, k);
            }
            cov.set(i, j, s);
        }
    }
    Some(LeastSquares {
        coefficients: c,
        unscaled_covariance: cov,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol * (1.0 + b.abs())
    }

    #[test]
    fn solve_solves_a_3x3_system() {
        let mut a = Matrix::zeros(3, 3);
        let vals = [2.0, 1.0, -1.0, -3.0, -1.0, 2.0, -2.0, 1.0, 2.0];
        for (i, v) in vals.iter().enumerate() {
            a.set(i / 3, i % 3, *v);
        }
        let x = solve(&a, &[8.0, -11.0, -3.0]).unwrap();
        assert!(approx(x[0], 2.0, 1e-12) && approx(x[1], 3.0, 1e-12) && approx(x[2], -1.0, 1e-12));
    }

    #[test]
    fn solve_refuses_a_singular_matrix() {
        let mut a = Matrix::zeros(2, 2);
        a.set(0, 0, 1.0);
        a.set(0, 1, 2.0);
        a.set(1, 0, 2.0);
        a.set(1, 1, 4.0);
        assert!(solve(&a, &[1.0, 2.0]).is_none());
        assert!(invert(&a).is_none());
    }

    #[test]
    fn invert_gives_the_identity() {
        let mut a = Matrix::zeros(2, 2);
        a.set(0, 0, 4.0);
        a.set(0, 1, 7.0);
        a.set(1, 0, 2.0);
        a.set(1, 1, 6.0);
        let inv = invert(&a).unwrap();
        assert!(approx(inv.get(0, 0), 0.6, 1e-12));
        assert!(approx(inv.get(0, 1), -0.7, 1e-12));
        assert!(approx(inv.get(1, 0), -0.2, 1e-12));
        assert!(approx(inv.get(1, 1), 0.4, 1e-12));
    }

    #[test]
    fn least_squares_recover_an_exact_line() {
        let xs = [0.0, 1.0, 2.0, 3.0, 4.0];
        let mut a = Matrix::zeros(5, 2);
        let mut y = Vec::new();
        for (i, x) in xs.iter().enumerate() {
            a.set(i, 0, 1.0);
            a.set(i, 1, *x);
            y.push(3.0 - 2.0 * x);
        }
        let ls = least_squares(&a, &y).unwrap();
        assert!(approx(ls.coefficients[0], 3.0, 1e-12));
        assert!(approx(ls.coefficients[1], -2.0, 1e-12));
        // (AᵀA)⁻¹ de x = 0..4 : [[0.6, -0.2], [-0.2, 0.1]].
        assert!(approx(ls.unscaled_covariance.get(0, 0), 0.6, 1e-10));
        assert!(approx(ls.unscaled_covariance.get(0, 1), -0.2, 1e-10));
        assert!(approx(ls.unscaled_covariance.get(1, 1), 0.1, 1e-10));
    }

    #[test]
    fn least_squares_refuse_an_insufficient_rank() {
        let mut a = Matrix::zeros(4, 2);
        for i in 0..4 {
            a.set(i, 0, 1.0);
            a.set(i, 1, 2.0);
        }
        assert!(least_squares(&a, &[1.0, 2.0, 3.0, 4.0]).is_none());
    }

    #[test]
    fn gram_and_transpose_mul_are_consistent() {
        let mut a = Matrix::zeros(3, 2);
        let vals = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        for (i, v) in vals.iter().enumerate() {
            a.set(i / 2, i % 2, *v);
        }
        let g = a.gram();
        assert!(approx(g.get(0, 0), 35.0, 1e-12));
        assert!(approx(g.get(0, 1), 44.0, 1e-12));
        assert!(approx(g.get(1, 0), 44.0, 1e-12));
        assert!(approx(g.get(1, 1), 56.0, 1e-12));
        let t = a.transpose_mul(&[1.0, 1.0, 1.0]);
        assert!(approx(t[0], 9.0, 1e-12) && approx(t[1], 12.0, 1e-12));
    }

    #[test]
    fn mul_vec_mul_and_transpose_on_rectangular_matrices() {
        // A (2x3) · B (3x2): known product, non-square shapes.
        let mut a = Matrix::zeros(2, 3);
        for (i, v) in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0].iter().enumerate() {
            a.set(i / 3, i % 3, *v);
        }
        let mut b = Matrix::zeros(3, 2);
        for (i, v) in [7.0, 8.0, 9.0, 10.0, 11.0, 12.0].iter().enumerate() {
            b.set(i / 2, i % 2, *v);
        }
        assert_eq!(a.mul_vec(&[1.0, 0.0, -1.0]), vec![-2.0, -2.0]);
        let p = a.mul(&b);
        assert_eq!((p.rows(), p.cols()), (2, 2));
        assert_eq!(
            [p.get(0, 0), p.get(0, 1), p.get(1, 0), p.get(1, 1)],
            [58.0, 64.0, 139.0, 154.0]
        );
        let t = a.transpose();
        assert_eq!((t.rows(), t.cols()), (3, 2));
        for i in 0..2 {
            for j in 0..3 {
                assert_eq!(t.get(j, i), a.get(i, j));
            }
        }
        assert_eq!(t.transpose(), a);
        // (A·B)ᵀ = Bᵀ·Aᵀ.
        assert_eq!(p.transpose(), b.transpose().mul(&a.transpose()));
        // The identity is neutral.
        assert_eq!(a.mul(&Matrix::identity(3)), a);
    }
}
