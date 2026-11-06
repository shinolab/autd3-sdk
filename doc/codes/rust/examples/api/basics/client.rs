use anyhow::Result;

use std::time::Duration;

use autd3_rs::commands::{Clear, Nop, Synchronize};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, Datagram, ExecAt, Frame, Frames};
use autd3_rs::udp::TransportOption;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = TransportOption::default();
    // ANCHOR: open
    let client = Client::open(&geometry, &option).await?;
    client.send(Clear).await?;
    client.send(Synchronize).await?;
    // ANCHOR_END: open

    // ANCHOR: exec_at
    let frames = Frames::encode(client.geometry(), Nop)?;

    let lead = ExecAt::Lead(Duration::from_millis(1));
    for frame in &frames {
        let msg_id = client.send_frame(frame, lead).await?;
        client.collect(msg_id).await?.check()?;
    }

    let at = client.device_time_now()? + Duration::from_millis(5);
    for frame in &frames {
        let msg_id = client.send_frame(frame, ExecAt::At(at)).await?;
        client.collect(msg_id).await?.check()?;
    }
    // ANCHOR_END: exec_at
    let frame = frames.iter().next().unwrap();
    // ANCHOR: heartbeat
    let mut heartbeat = tokio::time::interval(Duration::from_millis(10));
    for _ in 0..100 {
        heartbeat.tick().await;
        client.send_heartbeat().await?;
    }
    // ANCHOR_END: heartbeat
    // ANCHOR: api
    let num_devices = client.num_devices();
    let geometry = client.geometry();

    let msg_id = client.send_heartbeat().await?;
    let heartbeat = client.collect(msg_id).await?;
    let reply = client.recv().await?;

    let firmware = client.read_firmware_version().await?;
    let fpga_state = client.read_fpga_state().await?;

    client.send(Nop).await?;
    let msg_id = client.send_frame(frame, ExecAt::Immediate).await?;
    let resp = tokio::time::timeout(Duration::from_millis(200), client.collect(msg_id)).await??;

    // ANCHOR_END: api

    let _ = (num_devices, geometry, heartbeat, reply, firmware, fpga_state, resp);
    Ok(())
}
