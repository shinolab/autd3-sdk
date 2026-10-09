use std::collections::BTreeMap;
use std::net::SocketAddrV6;
use std::time::Duration;

use autd3_cpu_wire::udp::Kind;
use autd3_rs_core::Interface;
use if_addrs::IfAddr;

use super::channel::{Channel, all_nodes};
use super::error::UdpError;
use super::simulator::Simulator;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Candidate {
    name: String,
    scope: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) group: SocketAddrV6,
    pub(crate) scope: Option<u32>,
    pub(crate) simulator_control: Option<SocketAddrV6>,
}

impl Target {
    const fn simulator(simulator: Simulator) -> Self {
        Self {
            group: simulator.group,
            scope: None,
            simulator_control: Some(simulator.control),
        }
    }

    fn interface(candidate: &Candidate) -> Self {
        Self {
            group: all_nodes(candidate.scope),
            scope: Some(candidate.scope),
            simulator_control: None,
        }
    }
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

fn answers(target: Target, name: &str, response_timeout: Duration) -> bool {
    let result = (|| -> Result<bool, UdpError> {
        let channel = Channel::open(target.group, target.scope, None, None)?;
        Ok(channel
            .exchange(target.group, Kind::ReadUnitInfo, &[], response_timeout)?
            .is_some())
    })();
    match result {
        Ok(found) => found,
        Err(e @ UdpError::UnsupportedVersion { .. }) => {
            tracing::debug!(interface = %name, "an AUTD3 device answered: {e}");
            true
        }
        Err(e) => {
            tracing::debug!(interface = %name, "probing an interface failed: {e}");
            false
        }
    }
}

pub(crate) fn resolve_with(
    iface: &Interface,
    response_timeout: Duration,
    simulator: Simulator,
) -> Result<Target, UdpError> {
    let simulator = Target::simulator(simulator);
    match iface {
        Interface::Addr(addr) => Ok(Target {
            group: *addr,
            scope: (addr.scope_id() != 0).then_some(addr.scope_id()),
            simulator_control: None,
        }),
        Interface::Simulator => {
            if answers(simulator, "simulator", response_timeout) {
                Ok(simulator)
            } else {
                Err(UdpError::SimulatorNotFound(simulator.group))
            }
        }
        Interface::Name(name) => link_local_interfaces()?
            .iter()
            .find(|c| c.name == *name)
            .map(Target::interface)
            .ok_or_else(|| UdpError::InterfaceNotFound(name.clone())),
        _ => resolve_auto(response_timeout, simulator),
    }
}

fn resolve_auto(response_timeout: Duration, simulator: Target) -> Result<Target, UdpError> {
    let candidates = link_local_interfaces()?;
    let (simulator_answered, answered): (bool, Vec<bool>) = std::thread::scope(|s| {
        let simulator_probe = s.spawn(move || answers(simulator, "simulator", response_timeout));
        let probes: Vec<_> = candidates
            .iter()
            .map(|c| s.spawn(move || answers(Target::interface(c), &c.name, response_timeout)))
            .collect();
        (
            simulator_probe.join().unwrap_or(false),
            probes
                .into_iter()
                .map(|probe| probe.join().unwrap_or(false))
                .collect(),
        )
    });
    let answering: Vec<Candidate> = candidates
        .into_iter()
        .zip(answered)
        .filter_map(|(c, answered)| answered.then_some(c))
        .collect();
    if simulator_answered {
        if answering.is_empty() {
            tracing::info!(addr = %simulator.group, "found the simulator");
        } else {
            tracing::info!(
                addr = %simulator.group,
                ignored = ?answering.iter().map(|c| &c.name).collect::<Vec<_>>(),
                "found the simulator; it takes precedence over the AUTD3 devices that also answered"
            );
        }
        return Ok(simulator);
    }
    match answering.as_slice() {
        [] => Err(UdpError::NoInterfaceFound),
        [found] => {
            tracing::info!(interface = %found.name, "found AUTD3 devices");
            Ok(Target::interface(found))
        }
        _ => Err(UdpError::AmbiguousInterface(
            answering.into_iter().map(|c| c.name).collect(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::super::simulator::SIMULATOR;
    use super::*;

    const RESPONSE_TIMEOUT: Duration = Duration::from_millis(50);

    #[test]
    fn the_address_of_the_simulator_given_directly_is_not_a_simulator() {
        assert_eq!(
            resolve_with(
                &Interface::Addr(SIMULATOR.group),
                RESPONSE_TIMEOUT,
                SIMULATOR
            )
            .unwrap()
            .simulator_control,
            None
        );
    }

    #[test]
    fn a_scoped_address_sends_through_its_interface() {
        let scoped = all_nodes(7);
        assert_eq!(
            resolve_with(&Interface::Addr(scoped), RESPONSE_TIMEOUT, SIMULATOR).unwrap(),
            Target {
                group: scoped,
                scope: Some(7),
                simulator_control: None,
            }
        );
    }
}
