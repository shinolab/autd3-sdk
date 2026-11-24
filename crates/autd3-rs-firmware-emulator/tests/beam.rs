mod common;
#[path = "common/pattern.rs"]
mod pattern;

use autd3_cpu_wire::PatternBank;
use autd3_cpu_wire::fpga_params::REP_INFINITE;
use autd3_cpu_wire::payload::{EmissionType, TransitionModeKind, WriteFociPayload};
use autd3_rs_core::common::units::rad;
use autd3_rs_core::common::{Angle, Length};
use autd3_rs_core::geometry::{
    Autd3, Device, Geometry, Point3, UnitQuaternion, UnitVector3, Vector3,
};
use autd3_rs_core::protocol::{Cmd, DeviceErrorCode};
use autd3_rs_core::units::mm;
use autd3_rs_core::value::{Beam, ControlPoint, ControlPoints, Intensity, Phase};
use autd3_rs_pattern::{
    LaguerreGaussianOption, bessel_transducer, focus_transducer,
    laguerre_gaussian_phase_transducer, plane_transducer,
};
use zerocopy::IntoBytes;

use common::{NUM_TRANSDUCERS, frame};
use pattern::{activate_pattern_bank, config_pattern};

const BANK: PatternBank = PatternBank::B0;
const LAMBDA: Length = Length::from_mm(8.5);
const SOUND_SPEED: u16 = 340 * 64;

fn emit(device: &Device, sample: &ControlPoints) -> (Vec<Phase>, Vec<Intensity>) {
    let words: Vec<u64> = (0..sample.points.len())
        .map(|j| sample.focus(device, j).unwrap().encode().unwrap())
        .collect();
    emit_raw(&words, words.len(), 0)
}

fn emit_raw(words: &[u64], num_foci: usize, sample: usize) -> (Vec<Phase>, Vec<Intensity>) {
    let size = (words.len() / num_foci).max(2);
    let mut write = WriteFociPayload::new(BANK, 0, size * num_foci)
        .unwrap()
        .as_bytes()
        .to_vec();
    for w in words.iter().cycle().take(size * num_foci) {
        write.extend_from_slice(&w.to_le_bytes());
    }
    let config = config_pattern(
        BANK,
        EmissionType::Foci,
        u32::try_from(size).unwrap(),
        u8::try_from(num_foci).unwrap(),
        SOUND_SPEED,
        REP_INFINITE,
    );
    let mut emulator = autd3_rs_firmware_emulator::Device::new(NUM_TRANSDUCERS);
    for (cmd, payload) in [
        (Cmd::WriteFociBuffer, write),
        (Cmd::ConfigPattern, config),
        (
            Cmd::ActivatePatternBank,
            activate_pattern_bank(BANK, TransitionModeKind::Immediate),
        ),
    ] {
        assert_eq!(
            emulator.send(&frame(cmd, &payload)).status,
            DeviceErrorCode::None
        );
    }
    let sample_ns = u64::try_from(sample).unwrap() * 512 * 25_000;
    emulator.fpga_mut().update_with_sys_time(sample_ns);
    assert_eq!(emulator.fpga().current_pattern_idx(), sample);
    emulator.fpga().emissions()
}

fn word(kind: u64, x: i32, y: i32, z: i32, byte: u8) -> u64 {
    let field = |v: i32| u64::from(v.cast_unsigned()) & 0x3_FFFF;
    kind << 62 | u64::from(byte) << 54 | field(z) << 36 | field(y) << 18 | field(x)
}

fn checksum(phases: &[Phase]) -> u32 {
    phases
        .iter()
        .zip(1u32..)
        .fold(0u32, |acc, (p, i)| acc.wrapping_add(u32::from(p.0) * i))
}

#[test]
fn fixed_vectors_match_the_fpga_simulation_checksums() {
    let three = [
        word(0, 1000, 2000, 6000, 0xAA),
        word(3, 3000, 2500, 0x4002, 0x00),
        word(0, -4000, 3000, 8000, 0x30),
        word(1, 3000, 2600, 23000, 0x80),
        word(1, -2000, 100, 40000, 0x10),
        word(0, 500, 500, 4000, 0x55),
        word(2, 20000, -15000, -30000, 0x7F),
        word(0, 2000, 2000, 5000, 0x11),
        word(1, 1000, 1000, 30000, 0x22),
        word(3, -500, 7000, 0xC3FD, 0xFF),
        word(0, 3500, 2600, 6000, 0x00),
        word(1, 3456, 2640, 21760, 0xC0),
    ];
    let one = [
        word(2, -24000, 6000, 77777, 0x12),
        word(3, 3456, 2640, 0x0001, 0x34),
        word(1, 3456, 2640, 25000, 0x56),
        word(0, 3456, 2640, 6000, 0x78),
    ];
    let got: Vec<u32> = (0..4)
        .map(|s| checksum(&emit_raw(&three, 3, s).0))
        .chain((0..4).map(|s| checksum(&emit_raw(&one, 1, s).0)))
        .collect();
    assert_eq!(
        got,
        [
            0x003c_c3a0,
            0x003a_aee7,
            0x003a_bdad,
            0x003e_6aef,
            0x003c_626c,
            0x002d_4a45,
            0x0032_4e40,
            0x0039_f92d,
        ],
        "{got:08x?}"
    );
}

fn distance(a: Phase, b: Phase) -> u8 {
    let d = a.0.wrapping_sub(b.0);
    d.min(d.wrapping_neg())
}

fn max_error(actual: &[Phase], expected: &[Phase]) -> u8 {
    actual
        .iter()
        .zip(expected)
        .map(|(&a, &e)| distance(a, e))
        .max()
        .unwrap()
}

fn max_error_up_to_constant(actual: &[Phase], expected: &[Phase]) -> u8 {
    (0..=255u8)
        .map(|offset| {
            actual
                .iter()
                .zip(expected)
                .map(|(&a, &e)| distance(a, e + Phase(offset)))
                .max()
                .unwrap()
        })
        .min()
        .unwrap()
}

fn expected(device: &Device, f: impl Fn(Point3<f32>) -> Phase) -> Vec<Phase> {
    device.positions().iter().map(|&p| f(p)).collect()
}

fn devices() -> Vec<Device> {
    Geometry::new(vec![
        Autd3::default(),
        Autd3::new(
            Point3::new(-180.0, 40.0, 25.0),
            UnitQuaternion::from_axis_angle(&Vector3::z_axis(), 0.7),
        ),
        Autd3::new(
            Point3::new(60.0, -150.0, -10.0),
            UnitQuaternion::from_axis_angle(&Vector3::x_axis(), core::f32::consts::PI)
                * UnitQuaternion::from_axis_angle(&Vector3::z_axis(), -1.9),
        ),
    ])
    .iter()
    .cloned()
    .collect()
}

fn azimuth(axis: UnitVector3<f32>, r: Vector3<f32>) -> f32 {
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
    r.dot(&v).atan2(r.dot(&u))
}

#[test]
fn point_focus_matches_the_pattern_focus() {
    for device in devices() {
        for target in [
            Point3::new(90.0, 70.0, 150.0),
            Point3::new(-30.0, 10.0, 60.0),
            Point3::new(200.0, -80.0, 300.0),
        ] {
            let (phases, intensities) = emit(
                &device,
                &ControlPoints::new([ControlPoint::from(target)], Intensity(0x80)),
            );
            assert!(intensities.iter().all(|&i| i == Intensity(0x80)));
            let want = expected(&device, |p| focus_transducer(p, target, LAMBDA));
            assert!(
                max_error(&phases, &want) <= 3,
                "{}",
                max_error(&phases, &want)
            );
        }
    }
}

#[test]
fn bessel_matches_the_pattern_bessel_up_to_the_axial_constant() {
    let dev: Device = Autd3::default().into();
    let normal = dev.axial_direction();
    for (apex, theta, direction) in [
        (Point3::new(86.0, 66.0, 0.0), Angle::from_rad(0.3), normal),
        (
            Point3::new(10.0, 120.0, -50.0),
            Angle::from_rad(1.1),
            normal,
        ),
        (Point3::new(150.0, 20.0, 0.0), Angle::from_rad(0.6), -normal),
    ] {
        let beam = Beam::Bessel {
            apex,
            direction,
            theta,
            wavelength: LAMBDA,
            phase_offset: Phase::ZERO,
        };
        let (phases, _) = emit(&dev, &ControlPoints::from(beam));
        let want = expected(&dev, |p| {
            bessel_transducer(p, apex, direction, theta, LAMBDA)
        });
        let err = max_error_up_to_constant(&phases, &want);
        assert!(err <= 2, "{err}");
    }
}

#[test]
fn bessel_off_the_normal_is_rejected() {
    let dev: Device = Autd3::default().into();
    let beam = Beam::Bessel {
        apex: Point3::origin(),
        direction: UnitVector3::new_normalize(Vector3::new(0.1, 0.0, 1.0)),
        theta: Angle::from_rad(0.3),
        wavelength: LAMBDA,
        phase_offset: Phase::ZERO,
    };
    assert!(ControlPoints::from(beam).focus(&dev, 0).is_err());
}

#[test]
fn plane_wave_matches_the_pattern_plane_across_devices() {
    for device in devices() {
        for direction in [
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(0.3, -0.2, 1.0),
            Vector3::new(-1.0, 0.5, 0.2),
        ] {
            let direction = UnitVector3::new_normalize(direction);
            let beam = Beam::Plane {
                direction,
                wavelength: LAMBDA,
            };
            let (phases, intensities) = emit(&device, &ControlPoints::new([beam], Intensity(0x33)));
            assert!(intensities.iter().all(|&i| i == Intensity(0x33)));
            let want = expected(&device, |p| plane_transducer(p, direction, LAMBDA));
            assert!(
                max_error(&phases, &want) <= 1,
                "{}",
                max_error(&phases, &want)
            );
        }
    }
}

#[test]
fn vortex_on_a_focus_matches_the_vortex_phase_across_devices() {
    for device in devices() {
        let normal = device.axial_direction();
        let target = device.center() + normal.into_inner() * 150.0;
        for (axis, l) in [normal, -normal]
            .into_iter()
            .flat_map(|axis| [1i8, -2, 3].map(|l| (axis, l)))
        {
            let sample = ControlPoints::new(
                [
                    Beam::from(target),
                    Beam::Vortex {
                        center: target,
                        axis,
                        l,
                    },
                ],
                Intensity::MAX,
            );
            let (phases, _) = emit(&device, &sample);
            let want = expected(&device, |p| {
                focus_transducer(p, target, LAMBDA)
                    + Phase::from(f32::from(l) * azimuth(axis, p - target) * rad)
            });
            let err = max_error(&phases, &want);
            assert!(err <= 3 + l.unsigned_abs(), "l = {l}: {err}");
        }
    }
}

#[test]
fn vortex_on_a_focus_matches_the_pattern_laguerre_gaussian_up_to_a_constant() {
    let dev: Device = Autd3::default().into();
    let target = dev.center() + Vector3::new(0.0, 0.0, 150.0);
    let axis = Vector3::z_axis();
    for l in [1i8, -1, 2] {
        let sample = ControlPoints::new(
            [
                Beam::from(target),
                Beam::Vortex {
                    center: target,
                    axis,
                    l,
                },
            ],
            Intensity::MAX,
        );
        let (phases, _) = emit(&dev, &sample);
        let want = expected(&dev, |p| {
            laguerre_gaussian_phase_transducer(
                p,
                target,
                axis,
                LaguerreGaussianOption {
                    p: 0,
                    l: i32::from(l),
                    waist: 10.0 * mm,
                },
                LAMBDA,
            )
        });
        let err = max_error_up_to_constant(&phases, &want);
        assert!(err <= 3 + l.unsigned_abs(), "l = {l}: {err}");
    }
}

#[test]
fn superposed_terms_average_only_the_superposed_beams() {
    let dev: Device = Autd3::default().into();
    let target = dev.center() + Vector3::new(0.0, 0.0, 150.0);
    let alone = emit(&dev, &ControlPoints::from(target)).0;
    let direction = UnitVector3::new_normalize(Vector3::new(0.2, 0.1, 1.0));
    let with_plane = emit(
        &dev,
        &ControlPoints::new(
            [
                Beam::from(target),
                Beam::Plane {
                    direction,
                    wavelength: LAMBDA,
                },
            ],
            Intensity::MAX,
        ),
    )
    .0;
    let plane_alone = emit(
        &dev,
        &ControlPoints::from(Beam::Plane {
            direction,
            wavelength: LAMBDA,
        }),
    )
    .0;
    for ((&a, &b), &p) in alone.iter().zip(&with_plane).zip(&plane_alone) {
        assert_eq!(a + p, b);
    }
}
