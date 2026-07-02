use anyhow::Result;

use std::num::{NonZeroU32, NonZeroUsize};

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig, MAX_IN_FLIGHT};
use autd3_rs_link_nop::Nop;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let link = Nop;
    let option =
        // ANCHOR: config
        ClientConfig {
            timeout_cycles: 10,
            max_inflight: NonZeroUsize::new(MAX_IN_FLIGHT).unwrap(),
            send_interval_cycles: NonZeroU32::new(1).unwrap(),
            max_resync_rounds: NonZeroU32::new(8).unwrap(),
            low_latency: false,
            reset_resend_cycles: 2,
            rt_priority: None,
            rt_affinity: None,
            validate_state: true,
        }
        // ANCHOR_END: config
        ;
    // ANCHOR: api
    Client::open(&geometry, link, option).await?;
    // ANCHOR_END: api

    Ok(())
}
