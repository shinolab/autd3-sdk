use std::net::SocketAddr;

use anyhow::Result;

use autd3_rs::{RtSchedulePolicy, rt::default_rt_priority};
use autd3_rs_link_echocat::{EchocatLink, EchocatLinkOption};
use autd3_rs_link_remote::{
    BusOption, BusPacing, DeviceLayout, RemoteLinkError, RemoteServer, RemoteServerOption,
};

fn main() -> Result<()> {
    let bind: SocketAddr = "0.0.0.0:8080".parse()?;
    let pacing = BusPacing::default();
    let rt_priority = default_rt_priority();
    let rt_policy = RtSchedulePolicy::default();
    let rt_affinity = None;
    let stack_prefault_bytes = 0;
    let idle_timeout = std::time::Duration::from_secs(10);
    // ANCHOR: api
    let bus = BusOption {
        pacing,
        rt_priority,
        rt_policy,
        rt_affinity,
        stack_prefault_bytes,
        ..Default::default()
    };
    let option = RemoteServerOption {
        bind,
        bus,
        idle_timeout,
        ..Default::default()
    };
    RemoteServer::new(option, |_: &[DeviceLayout]| {
        EchocatLink::open(&EchocatLinkOption::default())
            .map_err(|e| RemoteLinkError::Link(e.to_string()))
    })?
    .serve()?;
    // ANCHOR_END: api

    Ok(())
}
