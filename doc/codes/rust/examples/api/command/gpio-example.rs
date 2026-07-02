use anyhow::Result;

use autd3_rs::commands::{GpioOut, SetGpioOut};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_nop::Nop;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let client = Client::open(&geometry, Nop, ClientConfig::default()).await?;

    let mut builder = client.datagram_builder();
    builder.push(SetGpioOut {
        outputs: [
            GpioOut::PatternBank,
            GpioOut::Thermo,
            GpioOut::PwmOut(0),
            GpioOut::Off,
        ],
    });
    let frames = builder.build()?;
    for frame in &frames {
        client.send_checked(frame).await?;
    }

    client.close().await?;
    Ok(())
}
