use core::f32::consts::PI;

use autd3_rs_core::{Autd3, Geometry, Point3, UnitQuaternion};
use autd3_rs_firmware_emulator::Device as EmuDevice;
use autd3_rs_simulator_protocol::{ServerMsg, TransState, TransducerInfo};

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

pub fn extract_states_into(devices: &[EmuDevice], out: &mut Vec<TransState>) {
    out.clear();
    for dev in devices {
        let fpga = dev.fpga();
        let modulation = fpga.modulation();
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
