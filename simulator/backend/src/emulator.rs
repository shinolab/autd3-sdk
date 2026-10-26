use core::f32::consts::PI;

use autd3_cpu_wire::fpga_params::GpioOutType;
use autd3_rs::MAX_DEVICES;
use autd3_rs::geometry::{Autd3, Geometry, Point3, Quaternion, UnitQuaternion};
use autd3_rs_firmware_emulator::{Device as EmuDevice, FpgaEmulator, pwm_levels};
use autd3_rs_simulator_protocol::{DevicePose, DeviceState, ServerMsg, TransState, TransducerInfo};
use thiserror::Error;

const _: () = assert!(autd3_rs_simulator_protocol::MAX_DEVICES == autd3_rs::MAX_DEVICES);
const _: () =
    assert!(autd3_rs_simulator_protocol::DEVICE_WIDTH == autd3_rs::geometry::Autd3::DEVICE_WIDTH);
const _: () = assert!(
    autd3_rs_simulator_protocol::ROTATION_NORM_TOLERANCE
        == autd3_rs::geometry::Autd3::ROTATION_NORM_TOLERANCE
);

const ULTRASOUND_PERIOD_COUNT: f32 = 512.0;
const GPIO_SAMPLES: usize = 512;
const _: () = assert!(GPIO_SAMPLES == autd3_rs::value::PULSE_WIDTH_PERIOD as usize);
const GPIO_VALUE_MASK: u64 = 0x00FF_FFFF_FFFF_FFFF;

fn gpio_type(raw: u64) -> Option<GpioOutType> {
    GpioOutType::from_u8((raw >> 56) as u8)
}

fn constant_wave(v: u8) -> Vec<u8> {
    vec![v; GPIO_SAMPLES]
}

fn pwm_waveform(fpga: &FpgaEmulator, tr: usize) -> Vec<u8> {
    let (phases, intensities) = fpga.emissions();
    let (Some(&em_phase), Some(&em_intensity)) = (phases.get(tr), intensities.get(tr)) else {
        return constant_wave(0);
    };
    let pw = fpga.to_pulse_width(em_intensity, fpga.modulation());
    pwm_levels(pw, em_phase.0).map(u8::from).collect()
}

fn gpio_waveform(fpga: &FpgaEmulator, raw: u64) -> Vec<u8> {
    let value = raw & GPIO_VALUE_MASK;
    match gpio_type(raw) {
        Some(GpioOutType::BaseSig) => (0..GPIO_SAMPLES)
            .map(|i| u8::from(i >= GPIO_SAMPLES / 2))
            .collect(),
        Some(GpioOutType::Thermo) => constant_wave(u8::from(fpga.is_thermo_asserted())),
        Some(GpioOutType::ForceFan) => constant_wave(u8::from(fpga.force_fan())),
        Some(GpioOutType::ModBank) => constant_wave(u8::from(fpga.current_mod_bank() == 1)),
        Some(GpioOutType::ModIdx) => constant_wave(u8::from(fpga.current_mod_idx() == 0)),
        Some(GpioOutType::PatternBank) => constant_wave(u8::from(fpga.current_pattern_bank() == 1)),
        Some(GpioOutType::PatternIdx) => constant_wave(u8::from(fpga.current_pattern_idx() == 0)),
        Some(GpioOutType::IsStmMode) => constant_wave(u8::from(
            fpga.pattern_cycle(fpga.current_pattern_bank()) != 1,
        )),
        Some(GpioOutType::SysTimeEq) => {
            let now = (fpga.sys_time() / 25_000) & GPIO_VALUE_MASK;
            constant_wave(u8::from(now == value))
        }
        Some(GpioOutType::PwmOut) => pwm_waveform(fpga, value as usize),
        Some(GpioOutType::Direct) => constant_wave(u8::from(value != 0)),
        Some(
            GpioOutType::None | GpioOutType::Sync | GpioOutType::SyncRaw | GpioOutType::SyncDiff,
        )
        | None => constant_wave(0),
        Some(_) => unreachable!(),
    }
}

#[must_use]
pub(crate) fn geometry_msg(geometry: &Geometry) -> ServerMsg {
    let transducers = geometry
        .iter()
        .flat_map(|dev| {
            let dir = dev.axial_direction();
            dev.positions().iter().map(move |pos| TransducerInfo {
                pos: [pos.x, pos.y, pos.z],
                dir: [dir.x, dir.y, dir.z],
            })
        })
        .collect();
    ServerMsg::Geometry {
        transducers,
        devices: device_poses(geometry),
    }
}

pub(crate) fn device_poses(geometry: &Geometry) -> Vec<DevicePose> {
    geometry
        .iter()
        .map(|dev| {
            let origin = dev.position(0);
            let rotation = dev.rotation();
            DevicePose {
                origin: [origin.x, origin.y, origin.z],
                rotation: [rotation.w, rotation.i, rotation.j, rotation.k],
            }
        })
        .collect()
}

#[derive(Debug, Error, PartialEq)]
pub(crate) enum GeometryError {
    #[error("the number of devices must be 1..={MAX_DEVICES}, but {0} were given")]
    DeviceCountOutOfRange(usize),
    #[error("device {device} has a non-finite origin or rotation")]
    NonFinite { device: usize },
    #[error("the rotation of device {device} must be a unit quaternion, but its norm is {norm}")]
    RotationNotUnit { device: usize, norm: f32 },
}

pub(crate) fn geometry_from_poses(poses: &[DevicePose]) -> Result<Geometry, GeometryError> {
    if !(1..=MAX_DEVICES).contains(&poses.len()) {
        return Err(GeometryError::DeviceCountOutOfRange(poses.len()));
    }
    poses
        .iter()
        .enumerate()
        .map(|(device, pose)| {
            let [w, x, y, z] = pose.rotation;
            if pose
                .origin
                .iter()
                .chain(&pose.rotation)
                .any(|v| !v.is_finite())
            {
                return Err(GeometryError::NonFinite { device });
            }
            let rotation = Quaternion::new(w, x, y, z);
            let norm = rotation.norm();
            if (norm - 1.0).abs() > Autd3::ROTATION_NORM_TOLERANCE {
                return Err(GeometryError::RotationNotUnit { device, norm });
            }
            Ok(Autd3::new(
                Point3::from(pose.origin),
                UnitQuaternion::from_quaternion(rotation),
            ))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Geometry::new)
}

pub(crate) fn extract_device_states(devices: &[&EmuDevice]) -> Vec<DeviceState> {
    devices
        .iter()
        .map(|dev| {
            let fpga = dev.fpga();
            let mod_bank = fpga.current_mod_bank();
            let pat_bank = fpga.current_pattern_bank();
            let fixed = fpga.silencer_fixed_update_rate_mode();
            let (intensity, phase) = if fixed {
                (
                    fpga.silencer_update_rate_intensity(),
                    fpga.silencer_update_rate_phase(),
                )
            } else {
                (
                    fpga.silencer_completion_steps_intensity(),
                    fpga.silencer_completion_steps_phase(),
                )
            };
            DeviceState {
                num_transducers: u16::try_from(fpga.num_transducers()).unwrap_or(u16::MAX),
                silencer_fixed_update_rate: fixed,
                silencer_intensity: intensity,
                silencer_phase: phase,
                mod_freq_div: fpga.modulation_freq_div(mod_bank),
                mod_cycle: u32::try_from(fpga.modulation_cycle(mod_bank)).unwrap_or(u32::MAX),
                mod_idx: u32::try_from(fpga.current_mod_idx()).unwrap_or(u32::MAX),
                mod_buffer: fpga.modulation_buffer(mod_bank),
                stm_freq_div: fpga.pattern_freq_div(pat_bank),
                stm_cycle: u32::try_from(fpga.pattern_cycle(pat_bank)).unwrap_or(u32::MAX),
                stm_idx: u32::try_from(fpga.current_pattern_idx()).unwrap_or(u32::MAX),
                gpio_types: std::array::from_fn(|i| {
                    gpio_type(fpga.gpio_out(i))
                        .map_or_else(|| "Unknown".to_string(), |ty| format!("{ty:?}"))
                }),
                gpio_out: std::array::from_fn(|i| gpio_waveform(fpga, fpga.gpio_out(i))),
            }
        })
        .collect()
}

pub(crate) fn extract_states(devices: &[&EmuDevice], mod_enabled: bool) -> Vec<TransState> {
    let mut out = Vec::new();
    for dev in devices {
        let fpga = dev.fpga();
        let modulation = if mod_enabled {
            fpga.modulation()
        } else {
            u8::MAX
        };
        let (phases, intensities) = fpga.emissions();
        for (i, (phase, &intensity)) in phases.iter().zip(&intensities).enumerate() {
            let pulse_width = fpga.to_pulse_width(intensity, modulation);
            let amp = (PI * f32::from(pulse_width) / ULTRASOUND_PERIOD_COUNT).sin();
            out.push(TransState {
                amp,
                phase: phase.rad(),
                enable: fpga.output_mask_enabled(i),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use core::num::NonZeroU16;

    use approx::assert_relative_eq;
    use autd3_rs::commands::{
        FixedUpdateRate, Modulation, Nop, Pattern, SetOutputMask, SetPulseWidthTable, SetSilencer,
    };
    use autd3_rs::geometry::{Autd3, Point3, UnitQuaternion};
    use autd3_rs::value::{Intensity, Phase, SamplingConfig};
    use autd3_rs_firmware_emulator::test_utils::FpgaEmulatorTestExt;
    use rstest::rstest;

    use crate::harness::Harness;

    const FULL_INTENSITY_PULSE_WIDTH: u16 = 256;

    fn raw(gpio_type: GpioOutType, value: u64) -> u64 {
        (u64::from(gpio_type.as_u8()) << 56) | value
    }

    fn driven(phase: Phase, modulation: u8) -> Harness {
        let mut h = Harness::new(1);
        h.send(SetPulseWidthTable::default());
        h.send(Modulation::new(
            SamplingConfig::new(NonZeroU16::MAX),
            &[modulation, modulation],
        ));
        let n = h.fpga().num_transducers();
        let phases = vec![vec![phase; n]];
        let intensities = vec![vec![Intensity(0xFF); n]];
        h.send(Pattern::new(&phases, &intensities));
        h
    }

    #[test]
    fn base_signal_gpio_is_a_square_wave() {
        let h = Harness::new(1);
        let wave = gpio_waveform(h.fpga(), raw(GpioOutType::BaseSig, 0));
        assert_eq!(wave.len(), GPIO_SAMPLES);
        assert!(wave[..GPIO_SAMPLES / 2].iter().all(|&v| v == 0));
        assert!(wave[GPIO_SAMPLES / 2..].iter().all(|&v| v == 1));
    }

    #[test]
    fn thermo_gpio_follows_the_fpga_flag() {
        let mut h = Harness::new(1);
        assert_eq!(
            gpio_waveform(h.fpga(), raw(GpioOutType::Thermo, 0)),
            constant_wave(0)
        );
        h.fpga_mut().set_thermal(true);
        assert_eq!(
            gpio_waveform(h.fpga(), raw(GpioOutType::Thermo, 0)),
            constant_wave(1)
        );
    }

    #[rstest]
    #[case::force_fan(GpioOutType::ForceFan, 0)]
    #[case::mod_bank(GpioOutType::ModBank, 0)]
    #[case::mod_idx(GpioOutType::ModIdx, 1)]
    #[case::pattern_bank(GpioOutType::PatternBank, 0)]
    #[case::pattern_idx(GpioOutType::PatternIdx, 1)]
    fn bank_and_index_gpios_report_the_current_selection(
        #[case] gpio_type: GpioOutType,
        #[case] expected: u8,
    ) {
        let h = Harness::new(1);
        assert_eq!(
            gpio_waveform(h.fpga(), raw(gpio_type, 0)),
            constant_wave(expected)
        );
    }

    #[test]
    fn is_stm_mode_gpio_is_low_for_a_single_pattern() {
        let h = driven(Phase(0), 0xFF);
        assert_eq!(
            gpio_waveform(h.fpga(), raw(GpioOutType::IsStmMode, 0)),
            constant_wave(0)
        );
    }

    #[test]
    fn sys_time_eq_gpio_compares_against_the_scaled_value() {
        let h = Harness::new(1);
        assert_eq!(
            gpio_waveform(h.fpga(), raw(GpioOutType::SysTimeEq, 0)),
            constant_wave(1)
        );
        assert_eq!(
            gpio_waveform(h.fpga(), raw(GpioOutType::SysTimeEq, 1)),
            constant_wave(0)
        );
    }

    #[test]
    fn direct_gpio_reports_the_raw_value() {
        let h = Harness::new(1);
        assert_eq!(
            gpio_waveform(h.fpga(), raw(GpioOutType::Direct, 0)),
            constant_wave(0)
        );
        assert_eq!(
            gpio_waveform(h.fpga(), raw(GpioOutType::Direct, 1)),
            constant_wave(1)
        );
    }

    #[test]
    fn unknown_gpio_type_is_low() {
        let h = Harness::new(1);
        assert_eq!(
            gpio_waveform(h.fpga(), (0x7F_u64 << 56) | 1),
            constant_wave(0)
        );
    }

    #[test]
    fn pwm_gpio_of_an_out_of_range_transducer_is_low() {
        let h = driven(Phase(0), 0xFF);
        let tr = h.fpga().num_transducers() as u64;
        assert_eq!(
            gpio_waveform(h.fpga(), raw(GpioOutType::PwmOut, tr)),
            constant_wave(0)
        );
    }

    #[test]
    fn pwm_gpio_wraps_around_the_period_boundary() {
        let h = driven(Phase(0), 0xFF);
        let wave = gpio_waveform(h.fpga(), raw(GpioOutType::PwmOut, 0));
        assert_eq!(wave.len(), GPIO_SAMPLES);
        let half = usize::from(FULL_INTENSITY_PULSE_WIDTH / 2);
        let rise = GPIO_SAMPLES - half;
        let fall = half;
        let expected: Vec<u8> = (0..GPIO_SAMPLES)
            .map(|i| u8::from(i < fall || rise <= i))
            .collect();
        assert_eq!(wave, expected);
    }

    #[test]
    fn pwm_gpio_is_shifted_by_the_phase() {
        let h = driven(Phase(64), 0xFF);
        let wave = gpio_waveform(h.fpga(), raw(GpioOutType::PwmOut, 0));
        let half = usize::from(FULL_INTENSITY_PULSE_WIDTH / 2);
        let expected: Vec<u8> = (0..GPIO_SAMPLES)
            .map(|i| u8::from(i < 128 + half))
            .collect();
        assert_eq!(wave, expected);
    }

    #[test]
    fn geometry_message_flattens_every_device() {
        let geometry = Geometry::new(vec![
            Autd3::default(),
            Autd3::new(Point3::new(0.0, 200.0, 0.0), UnitQuaternion::identity()),
        ]);
        let ServerMsg::Geometry {
            transducers,
            devices,
        } = geometry_msg(&geometry)
        else {
            panic!("expected a geometry message");
        };
        assert_eq!(devices, device_poses(&geometry));
        assert_eq!(transducers.len(), 2 * Autd3::NUM_TRANSDUCERS);
        let second = geometry.iter().nth(1).unwrap().position(0);
        assert_relative_eq!(
            transducers[Autd3::NUM_TRANSDUCERS].pos[..],
            [second.x, second.y, second.z][..]
        );
        assert_relative_eq!(transducers[0].dir[..], [0.0, 0.0, 1.0][..]);
    }

    #[test]
    fn device_poses_round_trip_through_the_geometry() {
        let geometry = Geometry::new(vec![
            Autd3::default(),
            Autd3::new(
                Point3::new(192.0, -10.5, 300.0),
                UnitQuaternion::from_euler_angles(0.3, -0.7, 1.1),
            ),
        ]);
        let poses = device_poses(&geometry);
        assert_eq!(poses[0].origin, [0.0; 3]);
        assert_eq!(poses[0].rotation, [1.0, 0.0, 0.0, 0.0]);
        let restored = geometry_from_poses(&poses).unwrap();
        assert_eq!(restored.num_devices(), 2);
        for (expected, actual) in geometry.iter().zip(restored.iter()) {
            assert_relative_eq!(expected.position(0), actual.position(0), epsilon = 1e-4);
            assert_relative_eq!(
                expected.rotation().angle_to(&actual.rotation()),
                0.0,
                epsilon = 1e-4
            );
        }
    }

    #[test]
    fn an_invalid_pose_is_refused() {
        let pose = DevicePose {
            origin: [0.0; 3],
            rotation: [2.0, 0.0, 0.0, 0.0],
        };
        assert!(matches!(
            geometry_from_poses(&[pose]),
            Err(GeometryError::RotationNotUnit { device: 0, .. })
        ));
        assert_eq!(
            geometry_from_poses(&[]).unwrap_err(),
            GeometryError::DeviceCountOutOfRange(0)
        );
        let identity = DevicePose {
            origin: [0.0; 3],
            rotation: [1.0, 0.0, 0.0, 0.0],
        };
        assert_eq!(
            geometry_from_poses(&vec![identity; MAX_DEVICES + 1]).unwrap_err(),
            GeometryError::DeviceCountOutOfRange(MAX_DEVICES + 1)
        );
        assert_eq!(
            geometry_from_poses(&[
                identity,
                DevicePose {
                    origin: [f32::INFINITY, 0.0, 0.0],
                    ..identity
                }
            ])
            .unwrap_err(),
            GeometryError::NonFinite { device: 1 }
        );
        assert_eq!(
            geometry_from_poses(&vec![identity; MAX_DEVICES])
                .unwrap()
                .num_devices(),
            MAX_DEVICES
        );
    }

    #[test]
    fn modulation_gating_only_changes_the_amplitude() {
        let mut h = driven(Phase(0), 0x00);
        let modulated = h.states();
        assert!(modulated.iter().all(|s| s.amp == 0.0));

        h.mod_enabled = false;
        h.send(Nop);
        let unmodulated = h.states();
        for state in &unmodulated {
            assert_relative_eq!(state.amp, 1.0);
        }
        assert_eq!(
            modulated.iter().map(|s| s.phase).collect::<Vec<_>>(),
            unmodulated.iter().map(|s| s.phase).collect::<Vec<_>>()
        );
    }

    #[test]
    fn device_state_reports_the_configured_pattern_and_modulation() {
        let h = driven(Phase(0), 0xFF);
        let states = h.device_states();
        assert_eq!(states.len(), 1);
        let state = &states[0];
        assert_eq!(
            usize::from(state.num_transducers),
            h.fpga().num_transducers()
        );
        assert_eq!(state.stm_cycle, 1);
        assert_eq!(state.stm_idx, 0);
        assert_eq!(state.mod_cycle, 2);
        assert_eq!(state.mod_buffer, vec![0xFF, 0xFF]);
        assert!(state.gpio_types.iter().all(|t| t == "None"));
        assert!(state.gpio_out.iter().all(|w| w == &constant_wave(0)));
    }

    #[test]
    fn every_transducer_of_every_device_has_a_state() {
        let mut h = Harness::new(2);
        h.send(Nop);
        let expected: usize = h.devices.iter().map(|d| d.fpga().num_transducers()).sum();
        assert_eq!(h.states().len(), expected);
        assert_eq!(h.device_states().len(), 2);
    }

    #[test]
    fn output_mask_is_reflected_in_the_state() {
        let mut h = Harness::new(1);
        h.send(Nop);
        assert!(h.states().iter().all(|s| s.enable));

        let num_transducers = h.fpga().num_transducers();
        let masks = vec![(0..num_transducers).map(|i| i % 2 == 0).collect::<Vec<_>>()];
        h.send(SetOutputMask { masks });

        let states = h.states();
        assert_eq!(states.len(), num_transducers);
        assert!(states.iter().step_by(2).all(|s| s.enable));
        assert!(states.iter().skip(1).step_by(2).all(|s| !s.enable));
    }

    #[test]
    fn silencer_mode_switches_the_reported_registers() {
        let mut h = Harness::new(1);
        h.send(Nop);
        assert!(!h.device_states()[0].silencer_fixed_update_rate);

        h.send(SetSilencer::new(FixedUpdateRate {
            intensity: NonZeroU16::new(3).unwrap(),
            phase: NonZeroU16::new(5).unwrap(),
        }));

        let state = h.device_states().into_iter().next().unwrap();
        assert!(state.silencer_fixed_update_rate);
        assert_eq!(state.silencer_intensity, 3);
        assert_eq!(state.silencer_phase, 5);
    }
}
