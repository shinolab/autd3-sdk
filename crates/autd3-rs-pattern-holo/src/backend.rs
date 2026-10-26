use nalgebra::{Complex, DMatrix, DVector};

use autd3_rs_core::geometry::{Point3, UnitVector3};
use autd3_rs_core::value::{Intensity, Phase};

use crate::amplitude_target::AmplitudeTarget;
use crate::constraint::IntensityConstraint;
use crate::directivity::Directivity;
use crate::propagation::{max_coefficient, phase_and_intensity, propagate};

pub trait LinAlgBackend {
    type Matrix;
    type Vector;

    fn make_vector(&self, data: Vec<Complex<f32>>) -> Self::Vector;
    fn vector_to_host(&self, v: &Self::Vector) -> Vec<Complex<f32>>;

    fn propagation_matrix(
        &self,
        tr_pos: &[Point3<f32>],
        tr_dir: &[UnitVector3<f32>],
        foci: &[AmplitudeTarget],
        wavenumber: f32,
        directivity: Directivity,
    ) -> Self::Matrix;

    fn back_prop(&self, g: &Self::Matrix) -> Self::Matrix;
    fn gemm(&self, a: &Self::Matrix, b: &Self::Matrix) -> Self::Matrix;
    fn gemv(&self, a: &Self::Matrix, x: &Self::Vector) -> Self::Vector;

    fn gemv_hadamard_normalized(
        &self,
        a: &Self::Matrix,
        x: Self::Vector,
        r: &Self::Vector,
    ) -> Self::Vector;

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

    fn amplitude_correct(&self, x: &mut Self::Vector, r: &Self::Vector);

    fn quantize(
        &self,
        v: &Self::Vector,
        constraint: IntensityConstraint,
    ) -> (Vec<Phase>, Vec<Intensity>);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NalgebraBackend;

impl LinAlgBackend for NalgebraBackend {
    type Matrix = DMatrix<Complex<f32>>;
    type Vector = DVector<Complex<f32>>;

    fn make_vector(&self, data: Vec<Complex<f32>>) -> Self::Vector {
        DVector::from_vec(data)
    }

    fn vector_to_host(&self, v: &Self::Vector) -> Vec<Complex<f32>> {
        v.as_slice().to_vec()
    }

    fn propagation_matrix(
        &self,
        tr_pos: &[Point3<f32>],
        tr_dir: &[UnitVector3<f32>],
        foci: &[AmplitudeTarget],
        wavenumber: f32,
        directivity: Directivity,
    ) -> Self::Matrix {
        propagation_matrix(tr_pos, tr_dir, foci, wavenumber, directivity)
    }

    fn back_prop(&self, g: &Self::Matrix) -> Self::Matrix {
        back_prop(g)
    }

    fn gemm(&self, a: &Self::Matrix, b: &Self::Matrix) -> Self::Matrix {
        a * b
    }

    fn gemv(&self, a: &Self::Matrix, x: &Self::Vector) -> Self::Vector {
        a * x
    }

    fn gemv_hadamard_normalized(
        &self,
        a: &Self::Matrix,
        mut x: Self::Vector,
        r: &Self::Vector,
    ) -> Self::Vector {
        hadamard_normalize(&mut x, r);
        a * x
    }

    fn amplitude_correct(&self, x: &mut Self::Vector, r: &Self::Vector) {
        amplitude_correct(x, r);
    }

    fn quantize(
        &self,
        v: &Self::Vector,
        constraint: IntensityConstraint,
    ) -> (Vec<Phase>, Vec<Intensity>) {
        quantized(v.as_slice(), constraint).unzip()
    }
}

fn propagation_matrix(
    tr_pos: &[Point3<f32>],
    tr_dir: &[UnitVector3<f32>],
    foci: &[AmplitudeTarget],
    wavenumber: f32,
    directivity: Directivity,
) -> DMatrix<Complex<f32>> {
    let m = foci.len();
    let n = tr_pos.len();
    let mut data = Vec::with_capacity(m * n);
    data.extend(tr_pos.iter().zip(tr_dir).flat_map(|(&pos, &dir)| {
        foci.iter()
            .map(move |f| propagate(pos, dir, f.point, wavenumber, directivity))
    }));
    DMatrix::from_vec(m, n, data)
}

fn back_prop(g: &DMatrix<Complex<f32>>) -> DMatrix<Complex<f32>> {
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

fn hadamard_normalize(x: &mut DVector<Complex<f32>>, r: &DVector<Complex<f32>>) {
    for (b, a) in x.as_mut_slice().iter_mut().zip(r.as_slice()) {
        let inv = 1.0 / (b.re * b.re + b.im * b.im).sqrt();
        let (re, im) = (b.re * inv, b.im * inv);
        *b = Complex::new(re * a.re - im * a.im, re * a.im + im * a.re);
    }
}

fn amplitude_correct(x: &mut DVector<Complex<f32>>, r: &DVector<Complex<f32>>) {
    for (b, a) in x.as_mut_slice().iter_mut().zip(r.as_slice()) {
        let inv = 1.0 / (b.re * b.re + b.im * b.im);
        let (re, im) = (b.re * inv, b.im * inv);
        let (ar, ai) = (a.re * a.re - a.im * a.im, 2.0 * a.re * a.im);
        *b = Complex::new(re * ar - im * ai, re * ai + im * ar);
    }
}

fn quantized(
    q: &[Complex<f32>],
    constraint: IntensityConstraint,
) -> impl Iterator<Item = (Phase, Intensity)> + '_ {
    let max = max_coefficient(q);
    q.iter()
        .map(move |&v| phase_and_intensity(v, constraint, max))
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Geometry, TransducerMask, Vector3};

    use super::*;
    use crate::amp::Pa;
    use crate::propagation::{enabled_transducers, target_amplitudes, wavenumber};
    use crate::test_utils::{geometry, wavelength};

    fn foci(g: &Geometry, nf: usize) -> Vec<AmplitudeTarget> {
        (0..nf)
            .map(|i| AmplitudeTarget {
                point: g.center() + Vector3::new(i as f32 * 10.0, i as f32 * -5.0, 150.0),
                amplitude: (3e3 + i as f32 * 200.0) * Pa,
            })
            .collect()
    }

    fn bits(v: &DVector<Complex<f32>>) -> Vec<(u32, u32)> {
        v.iter().map(|c| (c.re.to_bits(), c.im.to_bits())).collect()
    }

    type Matrix = DMatrix<Complex<f32>>;
    type Vector = DVector<Complex<f32>>;

    fn problem(geo: &Geometry, foci: &[AmplitudeTarget]) -> (Matrix, Matrix, Vector) {
        let b = NalgebraBackend;
        let (tr_pos, tr_dir) = enabled_transducers(geo, TransducerMask::AllEnabled);
        let g = b.propagation_matrix(
            &tr_pos,
            &tr_dir,
            foci,
            wavenumber(wavelength()),
            Directivity::Sphere,
        );
        let bp = b.back_prop(&g);
        (g, bp, target_amplitudes(&b, foci))
    }

    fn normalized(mut x: Vector, r: &Vector) -> Vector {
        hadamard_normalize(&mut x, r);
        x
    }

    #[test]
    fn fused_gs_is_bit_identical() {
        let b = NalgebraBackend;
        for (devices, nf, repeat) in [(1, 1, 1), (1, 4, 7), (2, 16, 100)] {
            let geo = geometry(devices);
            let foci = foci(&geo, nf);
            let (g, bp, amps) = problem(&geo, &foci);
            let n = TransducerMask::AllEnabled.num_enabled(&geo);
            let q0 = b.make_vector(vec![Complex::new(1.0, 0.0); n]);

            let mut want = q0.clone();
            for _ in 0..repeat {
                let p = normalized(b.gemv(&g, &normalized(want, &q0)), &amps);
                want = b.gemv(&bp, &p);
            }

            let mut got = q0.clone();
            for _ in 0..repeat {
                let p = b.gemv_hadamard_normalized(&g, got, &q0);
                got = b.gemv_hadamard_normalized(&bp, p, &amps);
            }

            assert_eq!(bits(&want), bits(&got), "gs {devices}dev/{nf}foci/{repeat}");
        }
    }

    #[test]
    fn fused_gspat_is_bit_identical() {
        let b = NalgebraBackend;
        for (devices, nf, repeat) in [(1, 1, 1), (1, 4, 7), (2, 16, 100)] {
            let geo = geometry(devices);
            let foci = foci(&geo, nf);
            let (g, bp, amps) = problem(&geo, &foci);
            let r = b.gemm(&g, &bp);

            let mut zeta = amps.clone();
            let mut want = amps.clone();
            for _ in 0..repeat {
                want = b.gemv(&r, &zeta);
                zeta = normalized(want.clone(), &amps);
            }
            b.amplitude_correct(&mut want, &amps);
            let want = b.gemv(&bp, &want);

            let mut got = b.repeat_gemv_normalized(&r, b.gemv(&r, &amps), &amps, repeat - 1);
            b.amplitude_correct(&mut got, &amps);
            let got = b.gemv(&bp, &got);

            assert_eq!(
                bits(&want),
                bits(&got),
                "gspat {devices}dev/{nf}foci/{repeat}"
            );
        }
    }
}
