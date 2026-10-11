use anyhow::Result;

use std::num::{NonZeroU32, NonZeroUsize};
use std::time::Duration;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::udp::DEVICE_QUEUE_FRAMES;
use autd3_rs::{Client, ClientConfig, TransportOption};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let udp = TransportOption::default();
    let option =
        // ANCHOR: config
        ClientConfig {
            ack_timeout: Duration::from_millis(10),
            max_inflight: NonZeroUsize::new(DEVICE_QUEUE_FRAMES).unwrap(),
            max_resync_rounds: NonZeroU32::new(8).unwrap(),
            require_supported_firmware: false,
            ..Default::default()
        }
        // ANCHOR_END: config
        ;
    // ANCHOR: api
    let client = Client::open(&geometry, &udp, option).await?;
    // ANCHOR_END: api

    Ok(())
}
