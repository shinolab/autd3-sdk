use autd3_rs_core::common::Length;
use autd3_rs_core::common::units::rad;
use autd3_rs_core::geometry::{Device, Geometry, Point3, UnitVector3};
use autd3_rs_core::value::{Emission, Phase};

use crate::focus::focus_phase;
use crate::gaussian::{
    AzimuthBasis, GaussianBeam, azimuth_basis, laguerre, write_intensity, write_intensity_device,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaguerreGaussianOption {
    pub p: u32,
    pub l: i32,
    pub waist: Length,
}

struct LaguerreGaussianMode {
    target: Point3<f32>,
    axis: UnitVector3<f32>,
    basis: AzimuthBasis,
    beam: GaussianBeam,
    p: u32,
    l: i32,
    wavelength: Length,
}

impl LaguerreGaussianMode {
    fn new(
        target: Point3<f32>,
        axis: UnitVector3<f32>,
        option: LaguerreGaussianOption,
        wavelength: Length,
    ) -> Self {
        Self {
            target,
            axis,
            basis: azimuth_basis(axis),
            beam: GaussianBeam::new(option.waist, wavelength),
            p: option.p,
            l: option.l,
            wavelength,
        }
    }

    fn alpha(&self) -> f32 {
        self.l.unsigned_abs() as f32
    }

    fn radial_argument(x: f32, y: f32, width_mm: f32) -> f32 {
        2.0 * x.mul_add(x, y * y) / (width_mm * width_mm)
    }

    fn phase(&self, position: Point3<f32>) -> Phase {
        let s = self
            .beam
            .sample(position, self.target, self.axis, &self.basis);
        let gouy_order = (2 * self.p) as f32 + self.alpha() + 1.0;
        let azimuth = s.y.atan2(s.x);
        let phase = Phase::from(
            focus_phase(s.offset, self.wavelength)
                + (self.l as f32 * azimuth - gouy_order * s.gouy) * rad,
        );
        let radial = laguerre(
            self.p,
            self.alpha(),
            Self::radial_argument(s.x, s.y, s.width_mm),
        );
        if radial < 0.0 {
            phase + Phase::PI
        } else {
            phase
        }
    }

    fn log_amplitude(&self, position: Point3<f32>) -> f32 {
        let s = self
            .beam
            .sample(position, self.target, self.axis, &self.basis);
        let arg = Self::radial_argument(s.x, s.y, s.width_mm);
        let alpha = self.alpha();
        let vortex_core = if self.l == 0 {
            0.0
        } else {
            0.5 * alpha * arg.ln()
        };
        s.log_waist_ratio + vortex_core + laguerre(self.p, alpha, arg).abs().ln() - 0.5 * arg
    }
}

#[must_use]
#[inline]
pub fn laguerre_gaussian_phase_transducer(
    position: Point3<f32>,
    target: Point3<f32>,
    axis: UnitVector3<f32>,
    option: LaguerreGaussianOption,
    wavelength: Length,
) -> Phase {
    LaguerreGaussianMode::new(target, axis, option, wavelength).phase(position)
}

pub fn laguerre_gaussian_phase_device(
    device: &Device,
    target: Point3<f32>,
    axis: UnitVector3<f32>,
    option: LaguerreGaussianOption,
    wavelength: Length,
    dst: &mut [Emission],
) {
    let mode = LaguerreGaussianMode::new(target, axis, option, wavelength);
    for (e, &pos) in dst.iter_mut().zip(device.positions()) {
        e.phase = mode.phase(pos);
    }
}

pub fn laguerre_gaussian_phase(
    geometry: &Geometry,
    target: Point3<f32>,
    axis: UnitVector3<f32>,
    option: LaguerreGaussianOption,
    wavelength: Length,
    dst: &mut [Vec<Emission>],
) {
    assert_eq!(
        dst.len(),
        geometry.num_devices(),
        "dst must have one slot per device"
    );
    for (slot, dev) in dst.iter_mut().zip(geometry.iter()) {
        laguerre_gaussian_phase_device(dev, target, axis, option, wavelength, slot);
    }
}

pub fn laguerre_gaussian_intensity_device(
    device: &Device,
    target: Point3<f32>,
    axis: UnitVector3<f32>,
    option: LaguerreGaussianOption,
    wavelength: Length,
    dst: &mut [Emission],
) {
    let mode = LaguerreGaussianMode::new(target, axis, option, wavelength);
    write_intensity_device(device, |pos| mode.log_amplitude(pos), dst);
}

pub fn laguerre_gaussian_intensity(
    geometry: &Geometry,
    target: Point3<f32>,
    axis: UnitVector3<f32>,
    option: LaguerreGaussianOption,
    wavelength: Length,
    dst: &mut [Vec<Emission>],
) {
    let mode = LaguerreGaussianMode::new(target, axis, option, wavelength);
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

    fn option(p: u32, l: i32) -> LaguerreGaussianOption {
        LaguerreGaussianOption {
            p,
            l,
            waist: 10.0 * mm,
        }
    }

    fn phase_distance(a: Phase, b: Phase) -> u8 {
        let d = a.0.wrapping_sub(b.0);
        d.min(d.wrapping_neg())
    }

    fn assert_constant_offset(actual: &[Phase], reference: &[Phase]) {
        let offset = Phase(actual[0].0.wrapping_sub(reference[0].0));
        for (&a, &r) in actual.iter().zip(reference) {
            assert!(
                phase_distance(a, r + offset) <= 2,
                "expected {a:?} ≈ {r:?} + {offset:?}"
            );
        }
    }

    #[test]
    fn p0_matches_the_vortex_phase_up_to_a_constant_on_a_perpendicular_plane() {
        let dev: Device = Autd3::default().into();
        let target = dev.center() + Vector3::new(0.0, 0.0, 150.0);
        let axis = Vector3::z_axis();
        for l in [-2_i32, -1, 1, 3] {
            let lg: Vec<_> = dev
                .positions()
                .iter()
                .map(|&pos| {
                    laguerre_gaussian_phase_transducer(pos, target, axis, option(0, l), LAMBDA)
                })
                .collect();
            let vortex: Vec<_> = dev
                .positions()
                .iter()
                .map(|&pos| {
                    let r = pos - target;
                    Phase::from(focus_phase(r, LAMBDA) + l as f32 * r.y.atan2(r.x) * rad)
                })
                .collect();
            assert_constant_offset(&lg, &vortex);
        }
    }

    #[test]
    fn azimuth_advances_by_l_times_the_angle() {
        let target = Point3::new(0.0, 0.0, 150.0);
        let axis = Vector3::z_axis();
        let radius = 40.0_f32;
        for l in [1_i32, 2, -1] {
            let base = laguerre_gaussian_phase_transducer(
                Point3::new(radius, 0.0, 0.0),
                target,
                axis,
                option(0, l),
                LAMBDA,
            );
            let quarter = laguerre_gaussian_phase_transducer(
                Point3::new(0.0, radius, 0.0),
                target,
                axis,
                option(0, l),
                LAMBDA,
            );
            let expected = base + Phase::from(l as f32 * core::f32::consts::FRAC_PI_2 * rad);
            assert!(phase_distance(quarter, expected) <= 1);
        }
    }

    #[test]
    fn fundamental_mode_matches_focus_up_to_a_constant() {
        let dev: Device = Autd3::default().into();
        let target = dev.center() + Vector3::new(0.0, 0.0, 150.0);
        let lg: Vec<_> = dev
            .positions()
            .iter()
            .map(|&pos| {
                laguerre_gaussian_phase_transducer(
                    pos,
                    target,
                    Vector3::z_axis(),
                    option(0, 0),
                    LAMBDA,
                )
            })
            .collect();
        let focus: Vec<_> = dev
            .positions()
            .iter()
            .map(|&pos| focus_transducer(pos, target, LAMBDA))
            .collect();
        assert_constant_offset(&lg, &focus);
    }

    #[test]
    fn radial_node_flips_the_phase_by_pi() {
        let target = Point3::new(0.0, 0.0, 150.0);
        let axis = Vector3::z_axis();
        let opt = option(1, 0);
        let beam = GaussianBeam::new(opt.waist, LAMBDA);
        let basis = azimuth_basis(axis);
        let width = beam.sample(Point3::origin(), target, axis, &basis).width_mm;
        let node = width / 2.0_f32.sqrt();

        let inner = Point3::new(node * 0.9, 0.0, 0.0);
        let outer = Point3::new(node * 1.1, 0.0, 0.0);
        let flipped = |pos: Point3<f32>| {
            laguerre_gaussian_phase_transducer(pos, target, axis, opt, LAMBDA)
                - laguerre_gaussian_phase_transducer(pos, target, axis, option(0, 0), LAMBDA)
        };
        let inner_shift = flipped(inner);
        let outer_shift = flipped(outer);
        assert!(phase_distance(outer_shift, inner_shift + Phase::PI) <= 1);
    }

    #[test]
    fn intensity_peaks_at_max_vanishes_on_axis_and_keeps_phase() {
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
        laguerre_gaussian_intensity_device(&dev, target, axis, option(0, 1), LAMBDA, &mut pattern);
        assert!(pattern.iter().all(|e| e.phase == Phase(0x42)));
        assert_eq!(
            pattern.iter().map(|e| e.intensity).max(),
            Some(Intensity::MAX)
        );

        let on_axis = Point3::new(target.x, target.y, 0.0);
        let mode = LaguerreGaussianMode::new(target, axis, option(0, 1), LAMBDA);
        let log_amplitude = mode.log_amplitude(on_axis);
        assert!(log_amplitude.is_infinite() && log_amplitude.is_sign_negative());
    }

    #[test]
    fn intensity_follows_the_gaussian_envelope() {
        let dev: Device = Autd3::default().into();
        let target = dev.center() + Vector3::new(0.0, 0.0, 150.0);
        let axis = Vector3::z_axis();
        let mut pattern = vec![Emission::NULL; Autd3::NUM_TRANSDUCERS];
        laguerre_gaussian_intensity_device(&dev, target, axis, option(0, 0), LAMBDA, &mut pattern);

        let rho = |i: usize| {
            let r = dev.position(i) - target;
            r.x.hypot(r.y)
        };
        let (nearest, _) = (0..Autd3::NUM_TRANSDUCERS)
            .map(|i| (i, rho(i)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        assert_eq!(pattern[nearest].intensity, Intensity::MAX);
        for i in 0..Autd3::NUM_TRANSDUCERS {
            for j in 0..Autd3::NUM_TRANSDUCERS {
                if rho(i) + 1.0 < rho(j) {
                    assert!(pattern[i].intensity >= pattern[j].intensity);
                }
            }
        }
    }

    #[test]
    fn underflowing_envelope_still_normalizes() {
        let dev: Device = Autd3::default().into();
        let target = dev.center() + Vector3::new(500.0, 0.0, 1.0);
        let opt = LaguerreGaussianOption {
            p: 2,
            l: 4,
            waist: 0.5 * mm,
        };
        let mut pattern = vec![Emission::NULL; Autd3::NUM_TRANSDUCERS];
        laguerre_gaussian_intensity_device(
            &dev,
            target,
            Vector3::z_axis(),
            opt,
            LAMBDA,
            &mut pattern,
        );
        assert_eq!(
            pattern.iter().map(|e| e.intensity).max(),
            Some(Intensity::MAX)
        );
    }

    #[test]
    fn device_level_matches_transducer_level_and_keeps_intensity() {
        let dev: Device = Autd3::default().into();
        let target = Point3::new(86.36, 66.04, 150.0);
        let axis = UnitVector3::new_normalize(Vector3::new(0.1, -0.2, 1.0));
        let mut pattern = vec![
            Emission {
                phase: Phase::ZERO,
                intensity: Intensity(0x42),
            };
            Autd3::NUM_TRANSDUCERS
        ];
        laguerre_gaussian_phase_device(&dev, target, axis, option(1, 2), LAMBDA, &mut pattern);
        for (i, &pos) in dev.positions().iter().enumerate() {
            assert_eq!(
                pattern[i].phase,
                laguerre_gaussian_phase_transducer(pos, target, axis, option(1, 2), LAMBDA)
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

        let mut emissions = geo.pattern_buffer();
        laguerre_gaussian_phase(&geo, target, axis, option(1, 1), LAMBDA, &mut emissions);
        let mut expected = geo.pattern_buffer();
        for (slot, dev) in expected.iter_mut().zip(&geo) {
            laguerre_gaussian_phase_device(dev, target, axis, option(1, 1), LAMBDA, slot);
        }
        assert_eq!(emissions, expected);
    }

    #[test]
    fn geometry_intensity_is_normalized_across_devices() {
        let geo = Geometry::new(vec![
            Autd3::default(),
            Autd3::new(Point3::new(200.0, 0.0, 0.0), UnitQuaternion::identity()),
        ]);
        let target = geo[0].center() + Vector3::new(0.0, 0.0, 150.0);
        let axis = Vector3::z_axis();

        let mut emissions = geo.pattern_buffer();
        laguerre_gaussian_intensity(&geo, target, axis, option(0, 0), LAMBDA, &mut emissions);
        let peak = |slot: &Vec<Emission>| slot.iter().map(|e| e.intensity).max().unwrap();
        assert_eq!(peak(&emissions[0]), Intensity::MAX);
        assert!(peak(&emissions[1]) < Intensity::MAX);

        let mut far = vec![Emission::NULL; Autd3::NUM_TRANSDUCERS];
        laguerre_gaussian_intensity_device(&geo[1], target, axis, option(0, 0), LAMBDA, &mut far);
        assert_eq!(peak(&far), Intensity::MAX);
    }
}
