use std::collections::BTreeMap;
use std::net::{Ipv6Addr, SocketAddrV6};
use std::time::Duration;

use autd3_cpu_wire::udp::{Kind, PORT};
use autd3_rs_core::Interface;
use if_addrs::IfAddr;

use super::channel::{Channel, all_nodes};
use super::error::UdpError;

const SIMULATOR: SocketAddrV6 = SocketAddrV6::new(Ipv6Addr::LOCALHOST, PORT, 0, 0);

#[derive(Clone, Debug, PartialEq, Eq)]
struct Candidate {
    name: String,
    scope: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) group: SocketAddrV6,
    pub(crate) scope: Option<u32>,
}

impl Target {
    const fn loopback(group: SocketAddrV6) -> Self {
        Self { group, scope: None }
    }

    fn interface(candidate: &Candidate) -> Self {
        Self {
            group: all_nodes(candidate.scope),
            scope: Some(candidate.scope),
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

pub(crate) fn resolve(iface: &Interface, response_timeout: Duration) -> Result<Target, UdpError> {
    resolve_with(iface, response_timeout, SIMULATOR)
}

fn resolve_with(
    iface: &Interface,
    response_timeout: Duration,
    simulator: SocketAddrV6,
) -> Result<Target, UdpError> {
    let simulator = Target::loopback(simulator);
    match iface {
        Interface::Addr(addr) => Ok(Target {
            group: *addr,
            scope: (addr.scope_id() != 0).then_some(addr.scope_id()),
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
    use std::net::UdpSocket;

    use autd3_rs_firmware_emulator::udp::UdpEmulator;

    use super::*;

    const RESPONSE_TIMEOUT: Duration = Duration::from_millis(50);

    fn vacant_loopback_addr() -> SocketAddrV6 {
        let socket = UdpSocket::bind((Ipv6Addr::LOCALHOST, 0)).unwrap();
        match socket.local_addr().unwrap() {
            std::net::SocketAddr::V6(addr) => addr,
            std::net::SocketAddr::V4(_) => unreachable!(),
        }
    }

    #[test]
    fn auto_prefers_an_answering_simulator() {
        let simulator = UdpEmulator::spawn(1).unwrap();
        assert_eq!(
            resolve_with(&Interface::Auto, RESPONSE_TIMEOUT, simulator.addr()).unwrap(),
            Target::loopback(simulator.addr())
        );
    }

    #[test]
    fn auto_never_picks_a_simulator_that_does_not_answer() {
        let vacant = vacant_loopback_addr();
        assert_ne!(
            resolve_with(&Interface::Auto, RESPONSE_TIMEOUT, vacant).ok(),
            Some(Target::loopback(vacant))
        );
    }

    #[test]
    fn simulator_resolves_to_an_answering_simulator() {
        let simulator = UdpEmulator::spawn(1).unwrap();
        assert_eq!(
            resolve_with(&Interface::Simulator, RESPONSE_TIMEOUT, simulator.addr()).unwrap(),
            Target::loopback(simulator.addr())
        );
    }

    #[test]
    fn simulator_fails_at_once_when_nothing_answers() {
        let vacant = vacant_loopback_addr();
        assert!(matches!(
            resolve_with(&Interface::Simulator, RESPONSE_TIMEOUT, vacant),
            Err(UdpError::SimulatorNotFound(addr)) if addr == vacant
        ));
    }

    #[test]
    fn an_address_is_used_without_probing() {
        let vacant = vacant_loopback_addr();
        assert_eq!(
            resolve_with(&Interface::Addr(vacant), RESPONSE_TIMEOUT, SIMULATOR).unwrap(),
            Target::loopback(vacant)
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
            }
        );
    }
}
