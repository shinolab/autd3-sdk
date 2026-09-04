use core::f32::consts::PI;

use autd3_rs_firmware_emulator::{Device as EmuDevice, FpgaEmulator};
use autd3_rs_link_remote::DeviceLayout;
use autd3_rs_simulator_protocol::{DeviceState, ServerMsg, TransState, TransducerInfo};

const ULTRASOUND_PERIOD_COUNT: f32 = 512.0;
const GPIO_SAMPLES: usize = 512;
const GPIO_PERIOD: u16 = 512;
const GPIO_VALUE_MASK: u64 = 0x00FF_FFFF_FFFF_FFFF;

const GPIO_TYPE_BASE_SIG: u8 = 0x01;
const GPIO_TYPE_THERMO: u8 = 0x02;
const GPIO_TYPE_FORCE_FAN: u8 = 0x03;
const GPIO_TYPE_MOD_BANK: u8 = 0x20;
const GPIO_TYPE_MOD_IDX: u8 = 0x21;
const GPIO_TYPE_PATTERN_BANK: u8 = 0x50;
const GPIO_TYPE_PATTERN_IDX: u8 = 0x51;
const GPIO_TYPE_IS_STM_MODE: u8 = 0x52;
const GPIO_TYPE_SYS_TIME_EQ: u8 = 0x60;
const GPIO_TYPE_PWM_OUT: u8 = 0xE0;
const GPIO_TYPE_DIRECT: u8 = 0xF0;

fn constant_wave(v: u8) -> Vec<u8> {
    vec![v; GPIO_SAMPLES]
}

fn pwm_waveform(fpga: &FpgaEmulator, tr: usize) -> Vec<u8> {
    let emissions = fpga.emissions();
    let Some(em) = emissions.get(tr) else {
        return constant_wave(0);
    };
    let pw = fpga.to_pulse_width(em.intensity, fpga.modulation());
    let phase = u16::from(em.phase.0) * 2;
    let rise = (GPIO_PERIOD + phase - pw / 2) % GPIO_PERIOD;
    let fall = (phase + pw / 2 + (pw & 0x01)) % GPIO_PERIOD;
    (0..GPIO_PERIOD)
        .map(|i| {
            let on = if rise <= fall {
                rise <= i && i < fall
            } else {
                i < fall || rise <= i
            };
            u8::from(on)
        })
        .collect()
}

fn gpio_waveform(fpga: &FpgaEmulator, raw: u64) -> Vec<u8> {
    let value = raw & GPIO_VALUE_MASK;
    match (raw >> 56) as u8 {
        GPIO_TYPE_BASE_SIG => (0..GPIO_SAMPLES)
            .map(|i| u8::from(i >= GPIO_SAMPLES / 2))
            .collect(),
        GPIO_TYPE_THERMO => constant_wave(u8::from(fpga.is_thermo_asserted())),
        GPIO_TYPE_FORCE_FAN => constant_wave(u8::from(fpga.force_fan())),
        GPIO_TYPE_MOD_BANK => constant_wave(u8::from(fpga.current_mod_bank() == 1)),
        GPIO_TYPE_MOD_IDX => constant_wave(u8::from(fpga.current_mod_idx() == 0)),
        GPIO_TYPE_PATTERN_BANK => constant_wave(u8::from(fpga.current_pattern_bank() == 1)),
        GPIO_TYPE_PATTERN_IDX => constant_wave(u8::from(fpga.current_pattern_idx() == 0)),
        GPIO_TYPE_IS_STM_MODE => constant_wave(u8::from(
            fpga.pattern_cycle(fpga.current_pattern_bank()) != 1,
        )),
        GPIO_TYPE_SYS_TIME_EQ => {
            let now = (fpga.sys_time() / 25_000) & GPIO_VALUE_MASK;
            constant_wave(u8::from(now == value))
        }
        GPIO_TYPE_PWM_OUT => pwm_waveform(fpga, value as usize),
        GPIO_TYPE_DIRECT => constant_wave(u8::from(value != 0)),
        _ => constant_wave(0),
    }
}

#[must_use]
pub fn geometry_msg_from_layout(layout: &[DeviceLayout]) -> ServerMsg {
    let transducers = layout
        .iter()
        .flat_map(|dev| {
            dev.transducers.iter().map(|t| TransducerInfo {
                pos: t.pos,
                dir: t.dir,
            })
        })
        .collect();
    ServerMsg::Geometry { transducers }
}

pub fn extract_device_states(devices: &[EmuDevice]) -> Vec<DeviceState> {
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
                gpio_types: std::array::from_fn(|i| (fpga.gpio_out(i) >> 56) as u8),
                gpio_out: std::array::from_fn(|i| gpio_waveform(fpga, fpga.gpio_out(i))),
            }
        })
        .collect()
}

pub fn extract_states_into(devices: &[EmuDevice], out: &mut Vec<TransState>, mod_enabled: bool) {
    out.clear();
    for dev in devices {
        let fpga = dev.fpga();
        let modulation = if mod_enabled {
            fpga.modulation()
        } else {
            u8::MAX
        };
        for (i, d) in fpga.emissions().iter().enumerate() {
            let pulse_width = fpga.to_pulse_width(d.intensity, modulation);
            let amp = (PI * f32::from(pulse_width) / ULTRASOUND_PERIOD_COUNT).sin();
            out.push(TransState {
                amp,
                phase: d.phase.rad(),
                enable: fpga.output_mask_enabled(i),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use core::num::NonZeroU16;

    use approx::assert_relative_eq;
    use autd3_rs::commands::{Modulation, Nop, Pattern, SetPulseWidthTable};
    use autd3_rs::value::{Emission, Intensity, Phase, SamplingConfig};
    use autd3_rs_link_remote::TransducerLayout;

    use crate::harness::Harness;

    const FULL_INTENSITY_PULSE_WIDTH: u16 = 256;

    fn raw(gpio_type: u8, value: u64) -> u64 {
        (u64::from(gpio_type) << 56) | value
    }

    fn driven(phase: Phase, modulation: u8) -> Harness {
        let mut h = Harness::new(1);
        let table = SetPulseWidthTable::default_table();
        h.send(SetPulseWidthTable { table: &table });
        h.send(Modulation::new(
            SamplingConfig::new(NonZeroU16::MAX),
            &[modulation, modulation],
        ));
        let emissions = vec![vec![
            Emission {
                phase,
                intensity: Intensity(0xFF),
            };
            h.fpga().num_transducers()
        ]];
        h.send(Pattern::new(&emissions));
        h
    }

    #[test]
    fn base_signal_gpio_is_a_square_wave() {
        let h = Harness::new(1);
        let wave = gpio_waveform(h.fpga(), raw(GPIO_TYPE_BASE_SIG, 0));
        assert_eq!(wave.len(), GPIO_SAMPLES);
        assert!(wave[..GPIO_SAMPLES / 2].iter().all(|&v| v == 0));
        assert!(wave[GPIO_SAMPLES / 2..].iter().all(|&v| v == 1));
    }

    #[test]
    fn thermo_gpio_follows_the_fpga_flag() {
        let mut h = Harness::new(1);
        assert_eq!(
            gpio_waveform(h.fpga(), raw(GPIO_TYPE_THERMO, 0)),
            constant_wave(0)
        );
        h.fpga_mut().set_thermal(true);
        assert_eq!(
            gpio_waveform(h.fpga(), raw(GPIO_TYPE_THERMO, 0)),
            constant_wave(1)
        );
    }

    #[test]
    fn bank_and_index_gpios_report_the_current_selection() {
        let h = Harness::new(1);
        for (gpio_type, expected) in [
            (GPIO_TYPE_FORCE_FAN, 0),
            (GPIO_TYPE_MOD_BANK, 0),
            (GPIO_TYPE_MOD_IDX, 1),
            (GPIO_TYPE_PATTERN_BANK, 0),
            (GPIO_TYPE_PATTERN_IDX, 1),
        ] {
            assert_eq!(
                gpio_waveform(h.fpga(), raw(gpio_type, 0)),
                constant_wave(expected),
                "gpio type {gpio_type:#04x}"
            );
        }
    }

    #[test]
    fn is_stm_mode_gpio_is_low_for_a_single_pattern() {
        let h = driven(Phase(0), 0xFF);
        assert_eq!(
            gpio_waveform(h.fpga(), raw(GPIO_TYPE_IS_STM_MODE, 0)),
            constant_wave(0)
        );
    }

    #[test]
    fn sys_time_eq_gpio_compares_against_the_scaled_value() {
        let h = Harness::new(1);
        assert_eq!(
            gpio_waveform(h.fpga(), raw(GPIO_TYPE_SYS_TIME_EQ, 0)),
            constant_wave(1)
        );
        assert_eq!(
            gpio_waveform(h.fpga(), raw(GPIO_TYPE_SYS_TIME_EQ, 1)),
            constant_wave(0)
        );
    }

    #[test]
    fn direct_gpio_reports_the_raw_value() {
        let h = Harness::new(1);
        assert_eq!(
            gpio_waveform(h.fpga(), raw(GPIO_TYPE_DIRECT, 0)),
            constant_wave(0)
        );
        assert_eq!(
            gpio_waveform(h.fpga(), raw(GPIO_TYPE_DIRECT, 1)),
            constant_wave(1)
        );
    }

    #[test]
    fn unknown_gpio_type_is_low() {
        let h = Harness::new(1);
        assert_eq!(gpio_waveform(h.fpga(), raw(0x7F, 1)), constant_wave(0));
    }

    #[test]
    fn pwm_gpio_of_an_out_of_range_transducer_is_low() {
        let h = driven(Phase(0), 0xFF);
        let tr = h.fpga().num_transducers() as u64;
        assert_eq!(
            gpio_waveform(h.fpga(), raw(GPIO_TYPE_PWM_OUT, tr)),
            constant_wave(0)
        );
    }

    #[test]
    fn pwm_gpio_wraps_around_the_period_boundary() {
        let h = driven(Phase(0), 0xFF);
        let wave = gpio_waveform(h.fpga(), raw(GPIO_TYPE_PWM_OUT, 0));
        assert_eq!(wave.len(), GPIO_PERIOD as usize);
        let half = FULL_INTENSITY_PULSE_WIDTH / 2;
        let rise = usize::from(GPIO_PERIOD - half);
        let fall = usize::from(half);
        let expected: Vec<u8> = (0..GPIO_PERIOD as usize)
            .map(|i| u8::from(i < fall || rise <= i))
            .collect();
        assert_eq!(wave, expected);
    }

    #[test]
    fn pwm_gpio_is_shifted_by_the_phase() {
        let h = driven(Phase(64), 0xFF);
        let wave = gpio_waveform(h.fpga(), raw(GPIO_TYPE_PWM_OUT, 0));
        let half = usize::from(FULL_INTENSITY_PULSE_WIDTH / 2);
        let expected: Vec<u8> = (0..GPIO_PERIOD as usize)
            .map(|i| u8::from(i < 128 + half))
            .collect();
        assert_eq!(wave, expected);
    }

    #[test]
    fn geometry_message_flattens_every_device() {
        let layout = vec![
            DeviceLayout {
                transducers: vec![
                    TransducerLayout {
                        pos: [0.0, 0.0, 0.0],
                        dir: [0.0, 0.0, 1.0],
                    },
                    TransducerLayout {
                        pos: [10.16, 0.0, 0.0],
                        dir: [0.0, 0.0, 1.0],
                    },
                ],
            },
            DeviceLayout {
                transducers: vec![TransducerLayout {
                    pos: [0.0, 200.0, 0.0],
                    dir: [0.0, 0.0, -1.0],
                }],
            },
        ];
        let ServerMsg::Geometry { transducers } = geometry_msg_from_layout(&layout) else {
            panic!("expected a geometry message");
        };
        assert_eq!(transducers.len(), 3);
        assert_relative_eq!(transducers[1].pos[..], [10.16, 0.0, 0.0][..]);
        assert_relative_eq!(transducers[2].dir[..], [0.0, 0.0, -1.0][..]);
    }

    #[test]
    fn geometry_message_of_an_empty_layout_is_empty() {
        let ServerMsg::Geometry { transducers } = geometry_msg_from_layout(&[]) else {
            panic!("expected a geometry message");
        };
        assert!(transducers.is_empty());
    }

    #[test]
    fn modulation_gating_only_changes_the_amplitude() {
        let mut h = driven(Phase(0), 0x00);
        let modulated = h.states();
        assert!(modulated.iter().all(|s| s.amp == 0.0));

        h.set_mod_enabled(false);
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
        assert_eq!(state.gpio_types, [0, 0, 0, 0]);
        assert!(state.gpio_out.iter().all(|w| w == &constant_wave(0)));
    }
}
