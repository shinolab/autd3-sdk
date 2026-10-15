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
    allow_simulator: bool,
) -> Result<Target, UdpError> {
    match iface {
        Interface::Addr(addr) => Ok(Target {
            group: *addr,
            scope: (addr.scope_id() != 0).then_some(addr.scope_id()),
            simulator_control: None,
        }),
        Interface::Simulator(_) if !allow_simulator => {
            Err(UdpError::UnsupportedInterface(iface.clone()))
        }
        Interface::Simulator(port) => {
            let simulator = Simulator::at(*port)
                .map(Target::simulator)
                .ok_or(UdpError::InvalidSimulatorPort(*port))?;
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
        _ => resolve_auto(
            response_timeout,
            allow_simulator.then_some(Target::simulator(Simulator::DEFAULT)),
        ),
    }
}

fn resolve_auto(response_timeout: Duration, simulator: Option<Target>) -> Result<Target, UdpError> {
    let candidates = link_local_interfaces()?;
    let (simulator_answered, answered): (bool, Vec<bool>) = std::thread::scope(|s| {
        let simulator_probe = simulator
            .map(|simulator| s.spawn(move || answers(simulator, "simulator", response_timeout)));
        let probes: Vec<_> = candidates
            .iter()
            .map(|c| s.spawn(move || answers(Target::interface(c), &c.name, response_timeout)))
            .collect();
        (
            simulator_probe.is_some_and(|probe| probe.join().unwrap_or(false)),
            probes
                .into_iter()
                .map(|probe| probe.join().unwrap_or(false))
                .collect(),
        )
    });
    let answering: Vec<(String, Target)> = simulator
        .filter(|_| simulator_answered)
        .map(|simulator| ("simulator".to_owned(), simulator))
        .into_iter()
        .chain(
            candidates
                .iter()
                .zip(answered)
                .filter(|&(_, answered)| answered)
                .map(|(c, _)| (c.name.clone(), Target::interface(c))),
        )
        .collect();
    pick_one(answering)
}

fn pick_one(answering: Vec<(String, Target)>) -> Result<Target, UdpError> {
    match answering.as_slice() {
        [] => Err(UdpError::NoInterfaceFound),
        [(name, target)] => {
            if target.simulator_control.is_some() {
                tracing::info!(addr = %target.group, "found the simulator");
            } else {
                tracing::info!(interface = %name, "found AUTD3 devices");
            }
            Ok(*target)
        }
        _ => Err(UdpError::AmbiguousInterface(
            answering.into_iter().map(|(name, _)| name).collect(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESPONSE_TIMEOUT: Duration = Duration::from_millis(50);

    #[test]
    fn the_address_of_the_simulator_given_directly_is_not_a_simulator() {
        assert_eq!(
            resolve_with(
                &Interface::Addr(Simulator::DEFAULT.group),
                RESPONSE_TIMEOUT,
                true
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
            resolve_with(&Interface::Addr(scoped), RESPONSE_TIMEOUT, true).unwrap(),
            Target {
                group: scoped,
                scope: Some(7),
                simulator_control: None,
            }
        );
    }

    fn interface(name: &str, scope: u32) -> (String, Target) {
        (
            name.to_owned(),
            Target::interface(&Candidate {
                name: name.to_owned(),
                scope,
            }),
        )
    }

    fn simulator() -> (String, Target) {
        (
            "simulator".to_owned(),
            Target::simulator(Simulator::DEFAULT),
        )
    }

    #[test]
    fn auto_picks_the_simulator_when_only_it_answers() {
        assert_eq!(
            pick_one(vec![simulator()]).unwrap(),
            Target::simulator(Simulator::DEFAULT)
        );
    }

    #[test]
    fn auto_picks_the_interface_when_only_it_answers() {
        assert_eq!(
            pick_one(vec![interface("eth0", 3)]).unwrap(),
            interface("eth0", 3).1
        );
    }

    #[test]
    fn auto_refuses_when_both_the_simulator_and_devices_answer() {
        assert!(matches!(
            pick_one(vec![simulator(), interface("eth0", 3)]),
            Err(UdpError::AmbiguousInterface(names)) if names == ["simulator", "eth0"]
        ));
    }

    #[test]
    fn auto_refuses_when_several_interfaces_answer() {
        assert!(matches!(
            pick_one(vec![interface("eth0", 3), interface("eth1", 4)]),
            Err(UdpError::AmbiguousInterface(names)) if names == ["eth0", "eth1"]
        ));
    }

    #[test]
    fn auto_fails_when_nothing_answers() {
        assert!(matches!(pick_one(vec![]), Err(UdpError::NoInterfaceFound)));
    }

    #[test]
    fn a_bus_for_the_devices_only_refuses_the_simulator() {
        assert!(matches!(
            resolve_with(&Interface::Simulator(44336), RESPONSE_TIMEOUT, false),
            Err(UdpError::UnsupportedInterface(Interface::Simulator(44336)))
        ));
    }

    #[test]
    fn a_simulator_port_without_a_control_port_is_refused() {
        for port in [0, u16::MAX] {
            assert!(matches!(
                resolve_with(&Interface::Simulator(port), RESPONSE_TIMEOUT, true),
                Err(UdpError::InvalidSimulatorPort(p)) if p == port
            ));
        }
    }

    #[test]
    fn the_simulator_at_a_port_controls_through_the_next_port() {
        let simulator = Simulator::at(50000).unwrap();
        assert_eq!(simulator.group.port(), 50000);
        assert_eq!(simulator.control.port(), 50001);
        assert_eq!(Simulator::DEFAULT.group.port(), 44336);
        assert_eq!(Simulator::DEFAULT.control.port(), 44337);
    }
}
