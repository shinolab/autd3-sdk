use anyhow::Result;

use std::time::Duration;

use autd3_rs::commands::Nop;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig, Datagram, ExecAt, Frame, Frames};
use autd3_rs::udp::TransportOption;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = TransportOption::default();
    let client = Client::open(&geometry, &option, ClientConfig::default()).await?;

    let frames = Frames::encode(&geometry, Nop)?;
    let frame = frames.iter().next().unwrap();

    // ANCHOR: exec_at
    let at = client.device_time_now()? + Duration::from_millis(5);
    client.send_frame_at(frame, ExecAt::At(at)).await?.await?.check()?;

    let lead = ExecAt::Lead(Duration::from_millis(1));
    client.send_frame_at(frame, lead).await?.await?.check()?;

    client.send_frame_at(frame, ExecAt::Immediate).await?.await?.check()?;
    // ANCHOR_END: exec_at
    // ANCHOR: api
    let num_devices = client.num_devices();
    let geometry = client.geometry();

    let config = client.config();
    client.set_config(ClientConfig {
        ack_timeout: Duration::from_millis(20),
        ..ClientConfig::default()
    })?;

    let firmware = client.read_firmware_version().await?;
    let fpga_state = client.read_fpga_state().await?;

    client.send(Nop).await?;
    let done = client.send_streaming(Nop).await?;
    let resp = client.send_frame(frame).await?.await?;

    client.close().await?;
    // ANCHOR_END: api

    let _ = (num_devices, geometry, config, firmware, fpga_state, done, resp);
    Ok(())
}
