use core::f32::consts::PI;

use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::{Device, Geometry, Point3, UnitVector3, Vector3};
use autd3_rs_core::value::{Emission, Intensity};

pub(crate) struct AzimuthBasis {
    pub(crate) u: Vector3<f32>,
    pub(crate) v: Vector3<f32>,
}

pub(crate) fn azimuth_basis(axis: UnitVector3<f32>) -> AzimuthBasis {
    let a = axis.into_inner();
    let seed = if a.x.abs() <= a.y.abs() && a.x.abs() <= a.z.abs() {
        Vector3::x()
    } else if a.y.abs() <= a.z.abs() {
        Vector3::y()
    } else {
        Vector3::z()
    };
    let u = (seed - a * a.dot(&seed)).normalize();
    let v = a.cross(&u);
    AzimuthBasis { u, v }
}

#[derive(Clone, Copy)]
pub(crate) struct GaussianBeam {
    waist_mm: f32,
    rayleigh_mm: f32,
}

pub(crate) struct BeamSample {
    pub(crate) offset: Vector3<f32>,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width_mm: f32,
    pub(crate) gouy: f32,
    pub(crate) log_waist_ratio: f32,
}

impl GaussianBeam {
    pub(crate) fn new(waist: Length, wavelength: Length) -> Self {
        let waist_mm = waist.mm();
        assert!(waist_mm > 0.0, "waist must be positive");
        Self {
            waist_mm,
            rayleigh_mm: PI * waist_mm * waist_mm / wavelength.mm(),
        }
    }

    pub(crate) fn sample(
        self,
        position: Point3<f32>,
        target: Point3<f32>,
        axis: UnitVector3<f32>,
        basis: &AzimuthBasis,
    ) -> BeamSample {
        let offset = position - target;
        let zeta = axis.dot(&offset) / self.rayleigh_mm;
        let spread = zeta.mul_add(zeta, 1.0).sqrt();
        BeamSample {
            offset,
            x: basis.u.dot(&offset),
            y: basis.v.dot(&offset),
            width_mm: self.waist_mm * spread,
            gouy: zeta.atan(),
            log_waist_ratio: -spread.ln(),
        }
    }
}

pub(crate) fn laguerre(p: u32, alpha: f32, x: f32) -> f32 {
    if p == 0 {
        return 1.0;
    }
    let mut prev = 1.0;
    let mut cur = 1.0 + alpha - x;
    for k in 1..p {
        let k = k as f32;
        let next = ((2.0 * k + 1.0 + alpha - x) * cur - (k + alpha) * prev) / (k + 1.0);
        prev = cur;
        cur = next;
    }
    cur
}

pub(crate) fn hermite(n: u32, x: f32) -> f32 {
    if n == 0 {
        return 1.0;
    }
    let mut prev = 1.0;
    let mut cur = 2.0 * x;
    for k in 1..n {
        let next = 2.0 * x * cur - 2.0 * k as f32 * prev;
        prev = cur;
        cur = next;
    }
    cur
}

fn max_log_amplitude(
    positions: &[Point3<f32>],
    log_amplitude: &impl Fn(Point3<f32>) -> f32,
) -> f32 {
    positions
        .iter()
        .map(|&p| log_amplitude(p))
        .fold(f32::NEG_INFINITY, f32::max)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn scaled_intensity(log_amplitude: f32, log_max: f32) -> Intensity {
    if !log_max.is_finite() {
        return Intensity::MIN;
    }
    let relative = (log_amplitude - log_max).exp();
    Intensity((relative * 255.0).round().clamp(0.0, 255.0) as u8)
}

fn write_scaled(
    device: &Device,
    log_amplitude: &impl Fn(Point3<f32>) -> f32,
    log_max: f32,
    dst: &mut [Emission],
) {
    for (e, &pos) in dst.iter_mut().zip(device.positions()) {
        e.intensity = scaled_intensity(log_amplitude(pos), log_max);
    }
}

pub(crate) fn write_intensity_device(
    device: &Device,
    log_amplitude: impl Fn(Point3<f32>) -> f32,
    dst: &mut [Emission],
) {
    let log_max = max_log_amplitude(device.positions(), &log_amplitude);
    write_scaled(device, &log_amplitude, log_max, dst);
}

pub(crate) fn write_intensity(
    geometry: &Geometry,
    log_amplitude: impl Fn(Point3<f32>) -> f32,
    dst: &mut [Vec<Emission>],
) {
    assert_eq!(
        dst.len(),
        geometry.num_devices(),
        "dst must have one slot per device"
    );
    let log_max = geometry
        .iter()
        .map(|dev| max_log_amplitude(dev.positions(), &log_amplitude))
        .fold(f32::NEG_INFINITY, f32::max);
    for (slot, dev) in dst.iter_mut().zip(geometry.iter()) {
        write_scaled(dev, &log_amplitude, log_max, slot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn azimuth_basis_is_orthonormal_and_deterministic() {
        for axis in [
            Vector3::x_axis(),
            Vector3::y_axis(),
            Vector3::z_axis(),
            UnitVector3::new_normalize(Vector3::new(1.0, 2.0, 3.0)),
            UnitVector3::new_normalize(Vector3::new(-0.3, 0.9, -0.1)),
        ] {
            let basis = azimuth_basis(axis);
            let again = azimuth_basis(axis);
            assert_eq!(basis.u, again.u);
            assert_eq!(basis.v, again.v);
            approx::assert_abs_diff_eq!(basis.u.norm(), 1.0, epsilon = 1.0e-5);
            approx::assert_abs_diff_eq!(basis.v.norm(), 1.0, epsilon = 1.0e-5);
            approx::assert_abs_diff_eq!(basis.u.dot(&basis.v), 0.0, epsilon = 1.0e-5);
            approx::assert_abs_diff_eq!(axis.dot(&basis.u), 0.0, epsilon = 1.0e-5);
            approx::assert_abs_diff_eq!(axis.dot(&basis.v), 0.0, epsilon = 1.0e-5);
        }
    }

    #[test]
    fn default_axis_uses_the_conventional_azimuth() {
        let basis = azimuth_basis(Vector3::z_axis());
        assert_eq!(basis.u, Vector3::x());
        assert_eq!(basis.v, Vector3::y());
    }

    #[test]
    fn laguerre_matches_closed_forms() {
        for &x in &[0.0_f32, 0.3, 1.0, 2.5, 4.0] {
            for &a in &[0.0_f32, 1.0, 3.0] {
                approx::assert_relative_eq!(laguerre(0, a, x), 1.0);
                approx::assert_relative_eq!(laguerre(1, a, x), 1.0 + a - x, epsilon = 1.0e-5);
                let l2 = f32::midpoint(x * x - 2.0 * (a + 2.0) * x, (a + 1.0) * (a + 2.0));
                approx::assert_relative_eq!(laguerre(2, a, x), l2, epsilon = 1.0e-4);
                let l3 = (-x * x * x + 3.0 * (a + 3.0) * x * x - 3.0 * (a + 2.0) * (a + 3.0) * x
                    + (a + 1.0) * (a + 2.0) * (a + 3.0))
                    / 6.0;
                approx::assert_relative_eq!(laguerre(3, a, x), l3, epsilon = 1.0e-4);
            }
        }
    }

    #[test]
    fn hermite_matches_closed_forms() {
        for &x in &[-2.0_f32, -0.5, 0.0, 0.7, 1.5] {
            approx::assert_relative_eq!(hermite(0, x), 1.0);
            approx::assert_relative_eq!(hermite(1, x), 2.0 * x);
            approx::assert_relative_eq!(hermite(2, x), 4.0 * x * x - 2.0, epsilon = 1.0e-5);
            approx::assert_relative_eq!(
                hermite(3, x),
                8.0 * x * x * x - 12.0 * x,
                epsilon = 1.0e-4
            );
        }
    }

    #[test]
    fn scaled_intensity_maps_the_peak_to_max_and_handles_degenerate_peaks() {
        assert_eq!(scaled_intensity(-3.0, -3.0), Intensity::MAX);
        assert_eq!(scaled_intensity(f32::NEG_INFINITY, -3.0), Intensity::MIN);
        assert_eq!(scaled_intensity(-3.0 + 0.25_f32.ln(), -3.0), Intensity(64));
        assert_eq!(
            scaled_intensity(f32::NEG_INFINITY, f32::NEG_INFINITY),
            Intensity::MIN
        );
        assert_eq!(scaled_intensity(f32::NAN, f32::NAN), Intensity::MIN);
    }

    #[test]
    #[should_panic(expected = "waist must be positive")]
    fn zero_waist_is_rejected() {
        let _ = GaussianBeam::new(Length::from_mm(0.0), Length::from_mm(8.5));
    }
}
