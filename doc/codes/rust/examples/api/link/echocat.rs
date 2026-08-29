use std::time::Duration;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig, Interface};
use autd3_rs_link_echocat::{EchocatLinkOption, FramePhase, SleepStrategy};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let iface = Interface::Auto;
    let sync0_period = Duration::from_millis(1);
    let frame_phase = FramePhase::Auto;
    let pdu_timeout = Duration::from_millis(100);
    let state_transition_timeout = Duration::from_secs(10);
    let dc_static_sync_iterations = 10_000;
    let dc_start_delay = Duration::from_millis(100);
    let sync_tolerance = Duration::from_micros(1);
    let sync_timeout = Duration::from_secs(10);
    let process_data_watchdog = Duration::from_millis(100);
    let sleep_strategy = SleepStrategy::Sleep;
    // ANCHOR: api
    EchocatLinkOption {
        iface,
        sync0_period,
        frame_phase,
        pdu_timeout,
        state_transition_timeout,
        dc_static_sync_iterations,
        dc_start_delay,
        sync_tolerance,
        sync_timeout,
        process_data_watchdog,
        sleep_strategy,
    };
    // ANCHOR_END: api

    // ANCHOR: frame_phase
    FramePhase::Auto;
    FramePhase::At(Duration::from_micros(500));
    // ANCHOR_END: frame_phase

    // ANCHOR: sleep_strategy
    SleepStrategy::Sleep;
    SleepStrategy::Spin {
        margin: Duration::from_micros(100),
    };
    // ANCHOR_END: sleep_strategy

    let _ = Client::open(
        &geometry,
        EchocatLinkOption::default(),
        ClientConfig::default(),
    );

    Ok(())
}
