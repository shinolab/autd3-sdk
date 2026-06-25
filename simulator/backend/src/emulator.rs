use core::f32::consts::PI;

use autd3_rs_core::{Autd3, Geometry, Point3, UnitQuaternion};
use autd3_rs_firmware_emulator::Device as EmuDevice;
use autd3_rs_simulator_protocol::{DeviceState, ServerMsg, TransState, TransducerInfo};

const ULTRASOUND_PERIOD_COUNT: f32 = 512.0;

#[must_use]
pub fn build_geometry(num_devices: usize) -> Geometry {
    let devices: Vec<Autd3> = (0..num_devices)
        .map(|i| {
            Autd3::new(
                Point3::new(i as f32 * Autd3::DEVICE_WIDTH, 0.0, 0.0),
                UnitQuaternion::identity(),
            )
        })
        .collect();
    Geometry::new(devices)
}

#[must_use]
pub fn geometry_msg(geometry: &Geometry) -> ServerMsg {
    let transducers = geometry
        .iter()
        .flat_map(|dev| {
            dev.positions()
                .iter()
                .zip(dev.directions())
                .map(|(p, d)| TransducerInfo {
                    pos: [p.x, p.y, p.z],
                    dir: [d.x, d.y, d.z],
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
        for (i, d) in fpga.drives().iter().enumerate() {
            let pulse_width = fpga.to_pulse_width(d.intensity, modulation);
            let amp = (PI * f32::from(pulse_width) / ULTRASOUND_PERIOD_COUNT).sin();
            out.push(TransState {
                amp,
                phase: d.phase.radian(),
                enable: fpga.output_mask_enabled(i),
            });
        }
    }
}
