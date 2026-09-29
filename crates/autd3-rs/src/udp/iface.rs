use std::collections::BTreeMap;
use std::time::Duration;

use autd3_cpu_wire::udp::Kind;
use autd3_rs_core::Interface;
use if_addrs::IfAddr;

use super::channel::{Channel, all_nodes};
use super::error::UdpError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) name: String,
    pub(crate) scope: u32,
}

fn link_local_interfaces() -> Result<Vec<Candidate>, UdpError> {
    let mut found = BTreeMap::new();
    for interface in if_addrs::get_if_addrs()? {
        let IfAddr::V6(addr) = &interface.addr else {
            continue;
        };
        if !addr.ip.is_unicast_link_local() {
            continue;
        }
        let Some(scope) = interface.index else {
            continue;
        };
        found.entry(scope).or_insert(interface.name);
    }
    Ok(found
        .into_iter()
        .map(|(scope, name)| Candidate { name, scope })
        .collect())
}

fn answers(candidate: &Candidate, response_timeout: Duration) -> bool {
    let result = (|| -> Result<bool, UdpError> {
        let mut channel = Channel::open(all_nodes(candidate.scope), Some(candidate.scope))?;
        let group = channel.group();
        let msg_id = channel.request(group, Kind::ReadUnitInfo, &[])?;
        Ok(!channel
            .collect(Kind::ReadUnitInfo, msg_id, response_timeout, Some(1))?
            .is_empty())
    })();
    match result {
        Ok(found) => found,
        Err(e) => {
            tracing::debug!(interface = %candidate.name, "probing an interface failed: {e}");
            false
        }
    }
}

pub(crate) fn resolve(
    iface: &Interface,
    response_timeout: Duration,
) -> Result<Candidate, UdpError> {
    let candidates = link_local_interfaces()?;
    if let Some(name) = iface.name() {
        return candidates
            .into_iter()
            .find(|c| c.name == name)
            .ok_or_else(|| UdpError::InterfaceNotFound(name.to_owned()));
    }
    let answered: Vec<bool> = std::thread::scope(|s| {
        let probes: Vec<_> = candidates
            .iter()
            .map(|c| s.spawn(move || answers(c, response_timeout)))
            .collect();
        probes
            .into_iter()
            .map(|probe| probe.join().unwrap_or(false))
            .collect()
    });
    let answering: Vec<Candidate> = candidates
        .into_iter()
        .zip(answered)
        .filter_map(|(c, answered)| answered.then_some(c))
        .collect();
    match answering.len() {
        0 => Err(UdpError::NoInterfaceFound),
        1 => {
            let found = answering.into_iter().next().expect("one candidate");
            tracing::info!(interface = %found.name, "found AUTD3 devices");
            Ok(found)
        }
        _ => Err(UdpError::AmbiguousInterface(
            answering.into_iter().map(|c| c.name).collect(),
        )),
    }
}
