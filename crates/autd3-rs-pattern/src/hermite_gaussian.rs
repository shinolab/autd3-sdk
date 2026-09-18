use core::f32::consts::SQRT_2;

use autd3_rs_core::common::Length;
use autd3_rs_core::common::units::rad;
use autd3_rs_core::geometry::{Device, Geometry, Point3, UnitVector3};
use autd3_rs_core::value::{Emission, Phase};

use crate::focus::focus_phase;
use crate::gaussian::{
    AzimuthBasis, GaussianBeam, azimuth_basis, hermite, write_intensity, write_intensity_device,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HermiteGaussianOption {
    pub m: u32,
    pub n: u32,
    pub waist: Length,
}

fn hermite_basis(axis: UnitVector3<f32>, x_dir: UnitVector3<f32>) -> AzimuthBasis {
    let a = axis.into_inner();
    (x_dir.into_inner() - a * a.dot(&x_dir))
        .try_normalize(1.0e-6)
        .map_or_else(
            || azimuth_basis(axis),
            |u| AzimuthBasis { u, v: a.cross(&u) },
        )
}

struct HermiteGaussianMode {
    target: Point3<f32>,
    axis: UnitVector3<f32>,
    basis: AzimuthBasis,
    beam: GaussianBeam,
    m: u32,
    n: u32,
    wavelength: Length,
}

impl HermiteGaussianMode {
    fn new(
        target: Point3<f32>,
        axis: UnitVector3<f32>,
        x_dir: UnitVector3<f32>,
        option: HermiteGaussianOption,
        wavelength: Length,
    ) -> Self {
        Self {
            target,
            axis,
            basis: hermite_basis(axis, x_dir),
            beam: GaussianBeam::new(option.waist, wavelength),
            m: option.m,
            n: option.n,
            wavelength,
        }
    }

    fn envelope(&self, x: f32, y: f32, width_mm: f32) -> (f32, f32) {
        let xi = SQRT_2 * x / width_mm;
        let eta = SQRT_2 * y / width_mm;
        (
            hermite(self.m, xi) * hermite(self.n, eta),
            0.5 * xi.mul_add(xi, eta * eta),
        )
    }

    fn phase(&self, position: Point3<f32>) -> Phase {
        let s = self
            .beam
            .sample(position, self.target, self.axis, &self.basis);
        let gouy_order = (self.m + self.n) as f32 + 1.0;
        let phase = Phase::from(focus_phase(s.offset, self.wavelength) - gouy_order * s.gouy * rad);
        let (polynomial, _) = self.envelope(s.x, s.y, s.width_mm);
        if polynomial < 0.0 {
            phase + Phase::PI
        } else {
            phase
        }
    }

    fn log_amplitude(&self, position: Point3<f32>) -> f32 {
        let s = self
            .beam
            .sample(position, self.target, self.axis, &self.basis);
        let (polynomial, decay) = self.envelope(s.x, s.y, s.width_mm);
        s.log_waist_ratio + polynomial.abs().ln() - decay
    }
}

#[must_use]
#[inline]
pub fn hermite_gaussian_phase_transducer(
    position: Point3<f32>,
    target: Point3<f32>,
    axis: UnitVector3<f32>,
    x_dir: UnitVector3<f32>,
    option: HermiteGaussianOption,
    wavelength: Length,
) -> Phase {
    HermiteGaussianMode::new(target, axis, x_dir, option, wavelength).phase(position)
}

pub fn hermite_gaussian_phase_device(
    device: &Device,
    target: Point3<f32>,
    axis: UnitVector3<f32>,
    x_dir: UnitVector3<f32>,
    option: HermiteGaussianOption,
    wavelength: Length,
    dst: &mut [Emission],
) {
    let mode = HermiteGaussianMode::new(target, axis, x_dir, option, wavelength);
    for (e, &pos) in dst.iter_mut().zip(device.positions()) {
        e.phase = mode.phase(pos);
    }
}

pub fn hermite_gaussian_phase(
    geometry: &Geometry,
    target: Point3<f32>,
    axis: UnitVector3<f32>,
    x_dir: UnitVector3<f32>,
    option: HermiteGaussianOption,
    wavelength: Length,
    dst: &mut [Vec<Emission>],
) {
    assert_eq!(
        dst.len(),
        geometry.num_devices(),
        "dst must have one slot per device"
    );
    for (slot, dev) in dst.iter_mut().zip(geometry.iter()) {
        hermite_gaussian_phase_device(dev, target, axis, x_dir, option, wavelength, slot);
    }
}

pub fn hermite_gaussian_intensity_device(
    device: &Device,
    target: Point3<f32>,
    axis: UnitVector3<f32>,
    x_dir: UnitVector3<f32>,
    option: HermiteGaussianOption,
    wavelength: Length,
    dst: &mut [Emission],
) {
    let mode = HermiteGaussianMode::new(target, axis, x_dir, option, wavelength);
    write_intensity_device(device, |pos| mode.log_amplitude(pos), dst);
}

pub fn hermite_gaussian_intensity(
    geometry: &Geometry,
    target: Point3<f32>,
    axis: UnitVector3<f32>,
    x_dir: UnitVector3<f32>,
    option: HermiteGaussianOption,
    wavelength: Length,
    dst: &mut [Vec<Emission>],
) {
    let mode = HermiteGaussianMode::new(target, axis, x_dir, option, wavelength);
    write_intensity(geometry, |pos| mode.log_amplitude(pos), dst);
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, UnitQuaternion, Vector3};
    use autd3_rs_core::units::mm;
    use autd3_rs_core::value::Intensity;

    use super::*;
    use crate::focus_transducer;

    const LAMBDA: Length = Length::from_mm(8.5);

    fn option(m: u32, n: u32) -> HermiteGaussianOption {
        HermiteGaussianOption {
            m,
            n,
            waist: 10.0 * mm,
        }
    }

    fn phase_distance(a: Phase, b: Phase) -> u8 {
        let d = a.0.wrapping_sub(b.0);
        d.min(d.wrapping_neg())
    }

    #[test]
    fn fundamental_mode_matches_focus_up_to_a_constant() {
        let dev: Device = Autd3::default().into();
        let target = dev.center() + Vector3::new(0.0, 0.0, 150.0);
        let axis = Vector3::z_axis();
        let first = dev.position(0);
        let offset = hermite_gaussian_phase_transducer(
            first,
            target,
            axis,
            Vector3::x_axis(),
            option(0, 0),
            LAMBDA,
        ) - focus_transducer(first, target, LAMBDA);
        for &pos in dev.positions() {
            let hg = hermite_gaussian_phase_transducer(
                pos,
                target,
                axis,
                Vector3::x_axis(),
                option(0, 0),
                LAMBDA,
            );
            assert!(phase_distance(hg, focus_transducer(pos, target, LAMBDA) + offset) <= 2);
        }
    }

    #[test]
    fn first_order_mode_is_a_twin_trap_across_the_node_plane() {
        let dev: Device = Autd3::default().into();
        let target = dev.center() + Vector3::new(0.0, 0.0, 150.0);
        let axis = Vector3::z_axis();
        let x_dir = Vector3::x_axis();
        let twin = |pos: Point3<f32>| {
            let base = focus_transducer(pos, target, LAMBDA);
            if pos.x < target.x {
                base + Phase::PI
            } else {
                base
            }
        };
        let first = dev.position(0);
        let offset =
            hermite_gaussian_phase_transducer(first, target, axis, x_dir, option(1, 0), LAMBDA)
                - twin(first);
        for &pos in dev.positions() {
            if (pos.x - target.x).abs() < 1.0e-3 {
                continue;
            }
            let hg =
                hermite_gaussian_phase_transducer(pos, target, axis, x_dir, option(1, 0), LAMBDA);
            assert!(phase_distance(hg, twin(pos) + offset) <= 2);
        }
    }

    #[test]
    fn x_dir_rotates_the_node_line() {
        let target = Point3::new(0.0, 0.0, 150.0);
        let axis = Vector3::z_axis();
        let x = Vector3::x_axis();
        let y = Vector3::y_axis();
        let d = 30.0_f32;
        let along_x = |x_dir| {
            hermite_gaussian_phase_transducer(
                Point3::new(d, 0.0, 0.0),
                target,
                axis,
                x_dir,
                option(1, 0),
                LAMBDA,
            ) - hermite_gaussian_phase_transducer(
                Point3::new(-d, 0.0, 0.0),
                target,
                axis,
                x_dir,
                option(1, 0),
                LAMBDA,
            )
        };
        assert_eq!(along_x(x), Phase::PI);
        assert_eq!(along_x(y), Phase::ZERO);
    }

    #[test]
    fn x_dir_is_projected_and_falls_back_when_parallel() {
        let axis = Vector3::z_axis();
        let tilted = UnitVector3::new_normalize(Vector3::new(1.0, 0.0, 5.0));
        let basis = hermite_basis(axis, tilted);
        approx::assert_abs_diff_eq!(basis.u, Vector3::x(), epsilon = 1.0e-6);
        approx::assert_abs_diff_eq!(basis.v, Vector3::y(), epsilon = 1.0e-6);

        let parallel = hermite_basis(axis, axis);
        let fallback = azimuth_basis(axis);
        assert_eq!(parallel.u, fallback.u);
        assert_eq!(parallel.v, fallback.v);
    }

    #[test]
    fn intensity_vanishes_on_the_node_plane_and_keeps_phase() {
        let dev: Device = Autd3::default().into();
        let target = dev.center() + Vector3::new(0.0, 0.0, 150.0);
        let axis = Vector3::z_axis();
        let mut pattern = vec![
            Emission {
                phase: Phase(0x42),
                intensity: Intensity::MIN,
            };
            Autd3::NUM_TRANSDUCERS
        ];
        hermite_gaussian_intensity_device(
            &dev,
            target,
            axis,
            Vector3::x_axis(),
            option(1, 0),
            LAMBDA,
            &mut pattern,
        );
        assert!(pattern.iter().all(|e| e.phase == Phase(0x42)));
        assert_eq!(
            pattern.iter().map(|e| e.intensity).max(),
            Some(Intensity::MAX)
        );

        let mode = HermiteGaussianMode::new(target, axis, Vector3::x_axis(), option(1, 0), LAMBDA);
        let on_node = Point3::new(target.x, target.y + 20.0, 0.0);
        let log_amplitude = mode.log_amplitude(on_node);
        assert!(log_amplitude.is_infinite() && log_amplitude.is_sign_negative());
    }

    #[test]
    fn device_level_matches_transducer_level_and_keeps_intensity() {
        let dev: Device = Autd3::default().into();
        let target = Point3::new(86.36, 66.04, 150.0);
        let axis = UnitVector3::new_normalize(Vector3::new(0.1, -0.2, 1.0));
        let x_dir = UnitVector3::new_normalize(Vector3::new(1.0, 1.0, 0.0));
        let mut pattern = vec![
            Emission {
                phase: Phase::ZERO,
                intensity: Intensity(0x42),
            };
            Autd3::NUM_TRANSDUCERS
        ];
        hermite_gaussian_phase_device(
            &dev,
            target,
            axis,
            x_dir,
            option(2, 1),
            LAMBDA,
            &mut pattern,
        );
        for (i, &pos) in dev.positions().iter().enumerate() {
            assert_eq!(
                pattern[i].phase,
                hermite_gaussian_phase_transducer(pos, target, axis, x_dir, option(2, 1), LAMBDA)
            );
            assert_eq!(pattern[i].intensity, Intensity(0x42));
        }
    }

    #[test]
    fn geometry_level_matches_device_level() {
        let geo = Geometry::new(vec![
            Autd3::default(),
            Autd3::new(Point3::new(200.0, 0.0, 0.0), UnitQuaternion::identity()),
        ]);
        let target = Point3::new(100.0, 66.0, 150.0);
        let axis = Vector3::z_axis();
        let x_dir = Vector3::x_axis();

        let mut emissions = geo.pattern_buffer();
        hermite_gaussian_phase(
            &geo,
            target,
            axis,
            x_dir,
            option(1, 1),
            LAMBDA,
            &mut emissions,
        );
        let mut expected = geo.pattern_buffer();
        for (slot, dev) in expected.iter_mut().zip(&geo) {
            hermite_gaussian_phase_device(dev, target, axis, x_dir, option(1, 1), LAMBDA, slot);
        }
        assert_eq!(emissions, expected);
    }
}
