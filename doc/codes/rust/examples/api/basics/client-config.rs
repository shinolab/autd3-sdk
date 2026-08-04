use anyhow::Result;

use std::num::{NonZeroU32, NonZeroUsize};

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{
    Client, ClientConfig, MAX_INFLIGHT, RtSchedulePolicy, ThreadPriority, ThreadPriorityValue,
};
use autd3_rs_link_nop::Nop;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let link = Nop;
    let option =
        // ANCHOR: config
        ClientConfig {
            timeout_cycles: NonZeroU32::new(10).unwrap(),
            max_inflight: NonZeroUsize::new(MAX_INFLIGHT).unwrap(),
            max_resync_rounds: NonZeroU32::new(8).unwrap(),
            low_latency: false,
            reset_resend_cycles: NonZeroU32::new(2).unwrap(),
            rt_priority: Some(ThreadPriority::Crossplatform(
                ThreadPriorityValue::try_from(80).unwrap(),
            )),
            rt_policy: RtSchedulePolicy::Fifo,
            rt_affinity: None,
            validate_state: true,
            require_supported_firmware: false,
            ..Default::default()
        }
        // ANCHOR_END: config
        ;
    // ANCHOR: api
    Client::open(&geometry, link, option).await?;
    // ANCHOR_END: api

    Ok(())
}
