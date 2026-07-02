use std::time::Duration;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig, Interface};
use autd3_rs_link_ethercrab::{EtherCrabLinkOption, EtherCrabLinkOptionFull};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let interface = Interface::Auto;
    let sync0_period = Duration::from_millis(1);
    let sync0_shift = Duration::from_millis(0);
    let sync_tolerance = Duration::from_micros(1);
    let sync_timeout = Duration::from_secs(10);
    // ANCHOR: api
    EtherCrabLinkOption {
        interface,
        sync0_period,
        sync0_shift,
        sync_tolerance,
        sync_timeout,
    };
    // ANCHOR_END: api

    let interface = Interface::Auto;
    let timeouts = ethercrab::Timeouts {
        state_transition: Duration::from_secs(10),
        pdu: Duration::from_secs(10),
        eeprom: Duration::from_secs(10),
        wait_loop_delay: Duration::from_millis(1),
        mailbox_echo: Duration::from_secs(10),
        mailbox_response: Duration::from_secs(10),
    };
    let main_device_config = ethercrab::MainDeviceConfig::default();
    let dc_configuration = ethercrab::subdevice_group::DcConfiguration {
        start_delay: Duration::from_millis(0),
        sync0_period,
        sync0_shift,
    };
    // ANCHOR: api_full
    EtherCrabLinkOptionFull {
        interface,
        timeouts,
        main_device_config,
        dc_configuration,
        sync_tolerance,
        sync_timeout,
    };
    // ANCHOR_END: api_full

    Ok(())
}
