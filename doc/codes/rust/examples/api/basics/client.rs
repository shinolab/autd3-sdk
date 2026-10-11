use anyhow::Result;

use autd3_rs::commands::Nop;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig, Frames};
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

    let frames = Frames::encode(&geometry, Nop)?;
    let frame = frames.iter().next().unwrap();

    // ANCHOR: api
    let num_devices = client.num_devices();
    let geometry = client.geometry();

    let firmware = client.read_firmware_version().await?;
    let fpga_state = client.read_fpga_state().await?;

    client.send(Nop).await?;
    let done = client.send_streaming(Nop).await?;
    let resp = client.send_frame(frame).await?.await?;

    client.silent_stop().await?;
    client.close().await?;
    // ANCHOR_END: api

    let _ = (num_devices, geometry, firmware, fpga_state, done, resp);
    Ok(())
}
