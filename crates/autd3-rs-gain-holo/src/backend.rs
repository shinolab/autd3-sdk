use nalgebra::{Complex, DMatrix, DVector};

pub trait LinAlgBackend {
    type Matrix;
    type Vector;

    fn make_matrix(&self, rows: usize, cols: usize, data: Vec<Complex<f32>>) -> Self::Matrix;
    fn make_vector(&self, data: Vec<Complex<f32>>) -> Self::Vector;
    fn clone_vector(&self, v: &Self::Vector) -> Self::Vector;
    fn vector_to_host(&self, v: &Self::Vector) -> Vec<Complex<f32>>;

    fn back_prop(&self, g: &Self::Matrix) -> Self::Matrix;
    fn gemm(&self, a: &Self::Matrix, b: &Self::Matrix) -> Self::Matrix;
    fn gemv(&self, a: &Self::Matrix, x: &Self::Vector) -> Self::Vector;
    fn hadamard_normalize(&self, x: &mut Self::Vector, r: &Self::Vector);
    fn amplitude_correct(&self, x: &mut Self::Vector, r: &Self::Vector);
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NalgebraBackend;

impl LinAlgBackend for NalgebraBackend {
    type Matrix = DMatrix<Complex<f32>>;
    type Vector = DVector<Complex<f32>>;

    fn make_matrix(&self, rows: usize, cols: usize, data: Vec<Complex<f32>>) -> Self::Matrix {
        DMatrix::from_vec(rows, cols, data)
    }

    fn make_vector(&self, data: Vec<Complex<f32>>) -> Self::Vector {
        DVector::from_vec(data)
    }

    fn clone_vector(&self, v: &Self::Vector) -> Self::Vector {
        v.clone()
    }

    fn vector_to_host(&self, v: &Self::Vector) -> Vec<Complex<f32>> {
        v.iter().copied().collect()
    }

    fn back_prop(&self, g: &Self::Matrix) -> Self::Matrix {
        let m = g.nrows();
        let n = g.ncols();
        let mut data = Vec::with_capacity(m * n);
        for i in 0..m {
            let denom: f32 = (0..n).map(|j| g[(i, j)].norm_sqr()).sum();
            let x = Complex::new(1.0 / denom, 0.0);
            for j in 0..n {
                data.push(g[(i, j)].conj() * x);
            }
        }
        DMatrix::from_vec(n, m, data)
    }

    fn gemm(&self, a: &Self::Matrix, b: &Self::Matrix) -> Self::Matrix {
        a * b
    }

    fn gemv(&self, a: &Self::Matrix, x: &Self::Vector) -> Self::Vector {
        a * x
    }

    fn hadamard_normalize(&self, x: &mut Self::Vector, r: &Self::Vector) {
        x.zip_apply(r, |b, a| *b = *b / b.norm() * a);
    }

    fn amplitude_correct(&self, x: &mut Self::Vector, r: &Self::Vector) {
        x.zip_apply(r, |b, a| *b = *b / b.norm_sqr() * (a * a));
    }
}
