use nalgebra::{Complex, DMatrix, DVector};

use autd3_rs_core::geometry::{Point3, UnitVector3};
use autd3_rs_core::value::Emission;

use crate::amplitude_target::AmplitudeTarget;
use crate::constraint::EmissionConstraint;
use crate::directivity::Directivity;
use crate::propagation::{emission, max_coefficient, propagate};

#[cfg(feature = "parallel")]
use rayon::prelude::*;

pub trait LinAlgBackend {
    type Matrix;
    type Vector;
    type BatchMatrix;
    type BatchVector;

    fn make_matrix(&self, rows: usize, cols: usize, data: Vec<Complex<f32>>) -> Self::Matrix;
    fn make_vector(&self, data: Vec<Complex<f32>>) -> Self::Vector;
    fn clone_vector(&self, v: &Self::Vector) -> Self::Vector;
    fn vector_to_host(&self, v: &Self::Vector) -> Vec<Complex<f32>>;

    fn propagation_matrix(
        &self,
        tr_pos: &[Point3<f32>],
        tr_dir: &[UnitVector3<f32>],
        foci: &[AmplitudeTarget],
        wavenumber: f32,
        directivity: Directivity,
    ) -> Self::Matrix {
        let m = foci.len();
        let n = tr_pos.len();
        let mut data = Vec::with_capacity(m * n);
        data.extend(tr_pos.iter().zip(tr_dir).flat_map(|(&pos, &dir)| {
            foci.iter()
                .map(move |f| propagate(pos, dir, f.point, wavenumber, directivity))
        }));
        self.make_matrix(m, n, data)
    }

    fn back_prop(&self, g: &Self::Matrix) -> Self::Matrix;
    fn gemm(&self, a: &Self::Matrix, b: &Self::Matrix) -> Self::Matrix;
    fn gemv(&self, a: &Self::Matrix, x: &Self::Vector) -> Self::Vector;
    fn hadamard_normalize(&self, x: &mut Self::Vector, r: &Self::Vector);
    fn amplitude_correct(&self, x: &mut Self::Vector, r: &Self::Vector);

    fn gemv_hadamard_normalized(
        &self,
        a: &Self::Matrix,
        mut x: Self::Vector,
        r: &Self::Vector,
    ) -> Self::Vector {
        self.hadamard_normalize(&mut x, r);
        self.gemv(a, &x)
    }

    fn repeat_gemv_normalized(
        &self,
        a: &Self::Matrix,
        mut x: Self::Vector,
        r: &Self::Vector,
        repeat: usize,
    ) -> Self::Vector {
        for _ in 0..repeat {
            x = self.gemv_hadamard_normalized(a, x, r);
        }
        x
    }

    fn quantize(
        &self,
        v: &Self::Vector,
        constraint: EmissionConstraint,
        parallel: bool,
    ) -> Vec<Emission>;

    fn quantize_batch(
        &self,
        v: &Self::BatchVector,
        constraint: EmissionConstraint,
        parallel: bool,
    ) -> Vec<Emission>;

    fn make_batch_vector(&self, batch: usize, data: Vec<Complex<f32>>) -> Self::BatchVector;
    fn batch_vector_to_host(&self, v: &Self::BatchVector) -> Vec<Complex<f32>>;

    fn batch_propagation_matrix(
        &self,
        tr_pos: &[Point3<f32>],
        tr_dir: &[UnitVector3<f32>],
        foci: &[AmplitudeTarget],
        batch: usize,
        wavenumber: f32,
        directivity: Directivity,
    ) -> Self::BatchMatrix;

    fn batch_back_prop(&self, g: &Self::BatchMatrix) -> Self::BatchMatrix;
    fn batch_gemm(&self, a: &Self::BatchMatrix, b: &Self::BatchMatrix) -> Self::BatchMatrix;
    fn batch_gemv(&self, a: &Self::BatchMatrix, x: &Self::BatchVector) -> Self::BatchVector;

    fn batch_gemv_hadamard_normalized(
        &self,
        a: &Self::BatchMatrix,
        x: Self::BatchVector,
        r: &Self::BatchVector,
    ) -> Self::BatchVector;

    fn batch_repeat_gemv_normalized(
        &self,
        a: &Self::BatchMatrix,
        mut x: Self::BatchVector,
        r: &Self::BatchVector,
        repeat: usize,
    ) -> Self::BatchVector {
        for _ in 0..repeat {
            x = self.batch_gemv_hadamard_normalized(a, x, r);
        }
        x
    }

    fn batch_amplitude_correct(&self, x: &mut Self::BatchVector, r: &Self::BatchVector);

    fn max_batch(&self, _bytes_per_problem: usize) -> usize {
        usize::MAX
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NalgebraBackend;

impl LinAlgBackend for NalgebraBackend {
    type Matrix = DMatrix<Complex<f32>>;
    type Vector = DVector<Complex<f32>>;
    type BatchMatrix = Vec<DMatrix<Complex<f32>>>;
    type BatchVector = Vec<DVector<Complex<f32>>>;

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
        data.extend((0..m).flat_map(|i| {
            let denom: f32 = (0..n).map(|j| g[(i, j)].norm_sqr()).sum();
            let x = Complex::new(1.0 / denom, 0.0);
            (0..n).map(move |j| g[(i, j)].conj() * x)
        }));
        DMatrix::from_vec(n, m, data)
    }

    fn gemm(&self, a: &Self::Matrix, b: &Self::Matrix) -> Self::Matrix {
        a * b
    }

    fn gemv(&self, a: &Self::Matrix, x: &Self::Vector) -> Self::Vector {
        a * x
    }

    fn hadamard_normalize(&self, x: &mut Self::Vector, r: &Self::Vector) {
        for (b, a) in x.as_mut_slice().iter_mut().zip(r.as_slice()) {
            let inv = 1.0 / (b.re * b.re + b.im * b.im).sqrt();
            let (re, im) = (b.re * inv, b.im * inv);
            *b = Complex::new(re * a.re - im * a.im, re * a.im + im * a.re);
        }
    }

    fn amplitude_correct(&self, x: &mut Self::Vector, r: &Self::Vector) {
        for (b, a) in x.as_mut_slice().iter_mut().zip(r.as_slice()) {
            let inv = 1.0 / (b.re * b.re + b.im * b.im);
            let (re, im) = (b.re * inv, b.im * inv);
            let (ar, ai) = (a.re * a.re - a.im * a.im, 2.0 * a.re * a.im);
            *b = Complex::new(re * ar - im * ai, re * ai + im * ar);
        }
    }

    fn quantize(
        &self,
        v: &Self::Vector,
        constraint: EmissionConstraint,
        parallel: bool,
    ) -> Vec<Emission> {
        let q = v.as_slice();
        quantize_slice(q, constraint, max_coefficient(q), parallel)
    }

    fn quantize_batch(
        &self,
        v: &Self::BatchVector,
        constraint: EmissionConstraint,
        parallel: bool,
    ) -> Vec<Emission> {
        #[cfg(not(feature = "parallel"))]
        let _ = parallel;
        #[cfg(feature = "parallel")]
        if parallel {
            return v
                .par_iter()
                .flat_map_iter(|q| quantized(q.as_slice(), constraint))
                .collect();
        }
        v.iter()
            .flat_map(|q| quantized(q.as_slice(), constraint))
            .collect()
    }

    fn make_batch_vector(&self, batch: usize, data: Vec<Complex<f32>>) -> Self::BatchVector {
        let batch = batch.max(1);
        let len = data.len() / batch;
        if len == 0 {
            return vec![DVector::zeros(0); batch];
        }
        data.chunks(len).map(DVector::from_column_slice).collect()
    }

    fn batch_vector_to_host(&self, v: &Self::BatchVector) -> Vec<Complex<f32>> {
        v.iter().flat_map(|v| v.iter().copied()).collect()
    }

    fn batch_propagation_matrix(
        &self,
        tr_pos: &[Point3<f32>],
        tr_dir: &[UnitVector3<f32>],
        foci: &[AmplitudeTarget],
        batch: usize,
        wavenumber: f32,
        directivity: Directivity,
    ) -> Self::BatchMatrix {
        foci.chunks(foci.len() / batch.max(1))
            .map(|foci| self.propagation_matrix(tr_pos, tr_dir, foci, wavenumber, directivity))
            .collect()
    }

    fn batch_back_prop(&self, g: &Self::BatchMatrix) -> Self::BatchMatrix {
        g.iter().map(|g| self.back_prop(g)).collect()
    }

    fn batch_gemm(&self, a: &Self::BatchMatrix, b: &Self::BatchMatrix) -> Self::BatchMatrix {
        a.iter().zip(b).map(|(a, b)| self.gemm(a, b)).collect()
    }

    fn batch_gemv(&self, a: &Self::BatchMatrix, x: &Self::BatchVector) -> Self::BatchVector {
        a.iter()
            .zip(broadcast(x, a.len()))
            .map(|(a, x)| self.gemv(a, x))
            .collect()
    }

    fn batch_gemv_hadamard_normalized(
        &self,
        a: &Self::BatchMatrix,
        x: Self::BatchVector,
        r: &Self::BatchVector,
    ) -> Self::BatchVector {
        a.iter()
            .zip(x)
            .zip(broadcast(r, a.len()))
            .map(|((a, x), r)| self.gemv_hadamard_normalized(a, x, r))
            .collect()
    }

    fn batch_amplitude_correct(&self, x: &mut Self::BatchVector, r: &Self::BatchVector) {
        let batch = x.len();
        for (x, r) in x.iter_mut().zip(broadcast(r, batch)) {
            self.amplitude_correct(x, r);
        }
    }
}

fn quantized(
    q: &[Complex<f32>],
    constraint: EmissionConstraint,
) -> impl Iterator<Item = Emission> + '_ {
    let max = max_coefficient(q);
    q.iter().map(move |&v| emission(v, constraint, max))
}

fn quantize_slice(
    q: &[Complex<f32>],
    constraint: EmissionConstraint,
    max: f32,
    parallel: bool,
) -> Vec<Emission> {
    #[cfg(not(feature = "parallel"))]
    let _ = parallel;
    #[cfg(feature = "parallel")]
    if parallel {
        return q
            .par_iter()
            .map(|&v| emission(v, constraint, max))
            .collect();
    }
    q.iter().map(|&v| emission(v, constraint, max)).collect()
}

fn broadcast<T>(v: &[T], batch: usize) -> impl Iterator<Item = &T> {
    let stride = usize::from(v.len() > 1);
    (0..batch).map(move |k| &v[k * stride])
}
