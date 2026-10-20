use std::collections::BTreeMap;
use std::net::{Ipv6Addr, SocketAddrV6};
use std::time::Duration;

use autd3_cpu_wire::udp::{ALL_NODES, Kind, PORT};
use autd3_rs_core::Interface;
use if_addrs::IfAddr;

use super::channel::Channel;
use super::error::UdpError;
use super::simulator::SimulatorAddrs;

#[derive(Clone, Debug, PartialEq, Eq)]
struct LinkLocalInterface {
    name: String,
    scope_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedEndpoint {
    pub(crate) all_devices_dst: SocketAddrV6,
    pub(crate) scope_id: Option<u32>,
    pub(crate) simulator_control: Option<SocketAddrV6>,
}

impl ResolvedEndpoint {
    const fn simulator(simulator: SimulatorAddrs) -> Self {
        Self {
            all_devices_dst: simulator.data,
            scope_id: None,
            simulator_control: Some(simulator.control),
        }
    }

    fn all_nodes_on(interface: &LinkLocalInterface) -> Self {
        Self {
            all_devices_dst: SocketAddrV6::new(
                Ipv6Addr::from(ALL_NODES),
                PORT,
                0,
                interface.scope_id,
            ),
            scope_id: Some(interface.scope_id),
            simulator_control: None,
        }
    }
}

fn link_local_interfaces() -> Result<Vec<LinkLocalInterface>, UdpError> {
    let mut found = BTreeMap::new();
    for interface in if_addrs::get_if_addrs()? {
        let IfAddr::V6(addr) = &interface.addr else {
            continue;
        };
        if !addr.ip.is_unicast_link_local() {
            continue;
        }
        let Some(scope_id) = interface.index else {
            continue;
        };
        found.entry(scope_id).or_insert(interface.name);
    }
    Ok(found
        .into_iter()
        .map(|(scope_id, name)| LinkLocalInterface { name, scope_id })
        .collect())
}

fn devices_respond_at(
    endpoint: ResolvedEndpoint,
    name: &str,
    control_request_timeout: Duration,
) -> bool {
    let result = (|| -> Result<bool, UdpError> {
        let channel = Channel::open(endpoint.all_devices_dst, endpoint.scope_id, None, None)?;
        Ok(channel
            .request_once(
                endpoint.all_devices_dst,
                Kind::ReadUnitInfo,
                &[],
                control_request_timeout,
            )?
            .is_some())
    })();
    match result {
        Ok(found) => found,
        Err(e @ UdpError::UnsupportedVersion { .. }) => {
            tracing::debug!(interface = %name, "an AUTD3 device answered: {e}");
            true
        }
        Err(e) => {
            tracing::debug!(interface = %name, "asking an interface for AUTD3 devices failed: {e}");
            false
        }
    }
}

pub(crate) fn resolve(
    iface: &Interface,
    control_request_timeout: Duration,
) -> Result<ResolvedEndpoint, UdpError> {
    match iface {
        Interface::Addr(addr) => Ok(ResolvedEndpoint {
            all_devices_dst: *addr,
            scope_id: (addr.scope_id() != 0).then_some(addr.scope_id()),
            simulator_control: None,
        }),
        Interface::Simulator(port) => {
            let simulator = SimulatorAddrs::at(*port)
                .map(ResolvedEndpoint::simulator)
                .ok_or(UdpError::InvalidSimulatorPort(*port))?;
            if devices_respond_at(simulator, "simulator", control_request_timeout) {
                Ok(simulator)
            } else {
                Err(UdpError::SimulatorNotFound(simulator.all_devices_dst))
            }
        }
        Interface::Name(name) => link_local_interfaces()?
            .iter()
            .find(|c| c.name == *name)
            .map(ResolvedEndpoint::all_nodes_on)
            .ok_or_else(|| UdpError::InterfaceNotFound(name.clone())),
        Interface::Auto => resolve_auto(
            control_request_timeout,
            ResolvedEndpoint::simulator(SimulatorAddrs::DEFAULT),
        ),
        _ => unreachable!(),
    }
}

fn resolve_auto(
    control_request_timeout: Duration,
    simulator: ResolvedEndpoint,
) -> Result<ResolvedEndpoint, UdpError> {
    let candidates = link_local_interfaces()?;
    let (simulator_answered, answered): (bool, Vec<bool>) = std::thread::scope(|s| {
        let simulator_probe =
            s.spawn(move || devices_respond_at(simulator, "simulator", control_request_timeout));
        let probes: Vec<_> = candidates
            .iter()
            .map(|c| {
                s.spawn(move || {
                    devices_respond_at(
                        ResolvedEndpoint::all_nodes_on(c),
                        &c.name,
                        control_request_timeout,
                    )
                })
            })
            .collect();
        (
            simulator_probe.join().unwrap_or(false),
            probes
                .into_iter()
                .map(|probe| probe.join().unwrap_or(false))
                .collect(),
        )
    });
    let answering: Vec<(String, ResolvedEndpoint)> = simulator_answered
        .then(|| ("simulator".to_owned(), simulator))
        .into_iter()
        .chain(
            candidates
                .iter()
                .zip(answered)
                .filter(|&(_, answered)| answered)
                .map(|(c, _)| (c.name.clone(), ResolvedEndpoint::all_nodes_on(c))),
        )
        .collect();
    pick_sole_responder(answering)
}

fn pick_sole_responder(
    answering: Vec<(String, ResolvedEndpoint)>,
) -> Result<ResolvedEndpoint, UdpError> {
    match answering.as_slice() {
        [] => Err(UdpError::NoInterfaceFound),
        [(name, target)] => {
            if target.simulator_control.is_some() {
                tracing::info!(addr = %target.all_devices_dst, "found the simulator");
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

    const CONTROL_REQUEST_TIMEOUT: Duration = Duration::from_millis(50);

    #[test]
    fn the_address_of_the_simulator_given_directly_is_not_a_simulator() {
        assert_eq!(
            resolve(
                &Interface::Addr(SimulatorAddrs::DEFAULT.data),
                CONTROL_REQUEST_TIMEOUT
            )
            .unwrap()
            .simulator_control,
            None
        );
    }

    #[test]
    fn a_scoped_address_sends_through_its_interface() {
        let scoped = SocketAddrV6::new(Ipv6Addr::from(ALL_NODES), PORT, 0, 7);
        assert_eq!(
            resolve(&Interface::Addr(scoped), CONTROL_REQUEST_TIMEOUT).unwrap(),
            ResolvedEndpoint {
                all_devices_dst: scoped,
                scope_id: Some(7),
                simulator_control: None,
            }
        );
    }

    fn interface(name: &str, scope_id: u32) -> (String, ResolvedEndpoint) {
        (
            name.to_owned(),
            ResolvedEndpoint::all_nodes_on(&LinkLocalInterface {
                name: name.to_owned(),
                scope_id,
            }),
        )
    }

    fn simulator() -> (String, ResolvedEndpoint) {
        (
            "simulator".to_owned(),
            ResolvedEndpoint::simulator(SimulatorAddrs::DEFAULT),
        )
    }

    #[test]
    fn auto_picks_the_simulator_when_only_it_answers() {
        assert_eq!(
            pick_sole_responder(vec![simulator()]).unwrap(),
            ResolvedEndpoint::simulator(SimulatorAddrs::DEFAULT)
        );
    }

    #[test]
    fn auto_picks_the_interface_when_only_it_answers() {
        assert_eq!(
            pick_sole_responder(vec![interface("eth0", 3)]).unwrap(),
            interface("eth0", 3).1
        );
    }

    #[test]
    fn auto_refuses_when_both_the_simulator_and_devices_answer() {
        assert!(matches!(
            pick_sole_responder(vec![simulator(), interface("eth0", 3)]),
            Err(UdpError::AmbiguousInterface(names)) if names == ["simulator", "eth0"]
        ));
    }

    #[test]
    fn auto_refuses_when_several_interfaces_answer() {
        assert!(matches!(
            pick_sole_responder(vec![interface("eth0", 3), interface("eth1", 4)]),
            Err(UdpError::AmbiguousInterface(names)) if names == ["eth0", "eth1"]
        ));
    }

    #[test]
    fn auto_fails_when_nothing_answers() {
        assert!(matches!(
            pick_sole_responder(vec![]),
            Err(UdpError::NoInterfaceFound)
        ));
    }

    #[test]
    fn a_simulator_port_without_a_control_port_is_refused() {
        for port in [0, u16::MAX] {
            assert!(matches!(
                resolve(&Interface::Simulator(port), CONTROL_REQUEST_TIMEOUT),
                Err(UdpError::InvalidSimulatorPort(p)) if p == port
            ));
        }
    }

    #[test]
    fn the_simulator_at_a_port_controls_through_the_next_port() {
        let simulator = SimulatorAddrs::at(50000).unwrap();
        assert_eq!(simulator.data.port(), 50000);
        assert_eq!(simulator.control.port(), 50001);
        assert_eq!(SimulatorAddrs::DEFAULT.data.port(), 44336);
        assert_eq!(SimulatorAddrs::DEFAULT.control.port(), 44337);
    }
}
