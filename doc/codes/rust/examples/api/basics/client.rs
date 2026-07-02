use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_nop::Nop;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let client = Client::open(&geometry, Nop, ClientConfig::default()).await?;

    let frames = client.datagram_builder().build()?;
    let frame = frames.iter().next().unwrap();

    // ANCHOR: api
    let num_devices = client.num_devices();

    let firmware = client.read_firmware_version().await?;
    let fpga_state = client.read_fpga_state().await?;
    let error_detail = client.read_error_detail().await?;

    let datagram_builder = client.datagram_builder();
    let resp = client.send(frame).await?.await?;
    client.send_checked(frame).await?;

    client.stop().await?;
    client.close().await?;
    // ANCHOR_END: api

    let _ = (num_devices, firmware, fpga_state, error_detail);
    Ok(())
}
