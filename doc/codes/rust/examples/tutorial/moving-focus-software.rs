use std::time::Duration;

use anyhow::Result;

use autd3_rs::commands::{Modulation, Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{Intensity, SamplingConfig};
use autd3_rs::{Client, ClientConfig};
use autd3_rs::udp::TransportOption;
use autd3_rs_firmware_emulator::udp::UdpEmulator;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let emulator = UdpEmulator::spawn(geometry.num_devices())?;
    let option = TransportOption {
        iface: emulator.interface(),
        ..TransportOption::default()
    };
    let client = Client::open(&geometry, &option, ClientConfig::default()).await?;

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);

    // ANCHOR: loop
    let mut phases = geometry.phase_buffer();
    loop {
        for sign in [1.0_f32, -1.0] {
            let target = center + offset((sign * 20.0) * mm, 0.0 * mm, 0.0 * mm);
            autd3_rs_pattern::focus(
                &geometry,
                target,
                wavelength,
                &mut phases,
            );
            client.send(Pattern::new(&phases, Intensity::MAX)).await?;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
    // ANCHOR_END: loop
}
