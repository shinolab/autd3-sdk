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
        // None / Sync / SyncDiff and unknown tags output constant low.
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
