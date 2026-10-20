use autd3_cpu_wire::udp::Kind;
use autd3_rs_core::Interface;

use crate::udp::{Channel, ResolvedEndpoint, SimulatorAddrs, UdpError, link_local_interfaces};

use super::requester::{REQUEST_REPLY_WAIT, Requester};

fn devices_respond_at(endpoint: ResolvedEndpoint, name: &str) -> bool {
    let result = (|| -> Result<bool, UdpError> {
        let channel = Channel::open(endpoint.all_devices_dst, endpoint.scope_id, None, None)?;
        Ok(Requester::new(&channel)
            .request_once(
                endpoint.all_devices_dst,
                Kind::ReadUnitInfo,
                &[],
                REQUEST_REPLY_WAIT,
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

pub(super) fn resolve(iface: &Interface) -> Result<ResolvedEndpoint, UdpError> {
    match iface {
        Interface::Addr(addr) => Ok(ResolvedEndpoint::at(*addr)),
        Interface::Simulator(port) => {
            let simulator = SimulatorAddrs::at(*port)
                .map(ResolvedEndpoint::simulator)
                .ok_or(UdpError::InvalidSimulatorPort(*port))?;
            if devices_respond_at(simulator, "simulator") {
                Ok(simulator)
            } else {
                Err(UdpError::SimulatorNotFound(simulator.all_devices_dst))
            }
        }
        Interface::Name(name) => link_local_interfaces()?
            .iter()
            .find(|c| c.name == *name)
            .map(|c| ResolvedEndpoint::all_nodes_on(c.scope_id))
            .ok_or_else(|| UdpError::InterfaceNotFound(name.clone())),
        Interface::Auto => resolve_auto(ResolvedEndpoint::simulator(SimulatorAddrs::DEFAULT)),
        _ => unreachable!(),
    }
}

fn resolve_auto(simulator: ResolvedEndpoint) -> Result<ResolvedEndpoint, UdpError> {
    let candidates = link_local_interfaces()?;
    let (simulator_answered, answered): (bool, Vec<bool>) = std::thread::scope(|s| {
        let simulator_probe = s.spawn(move || devices_respond_at(simulator, "simulator"));
        let probes: Vec<_> = candidates
            .iter()
            .map(|c| {
                s.spawn(move || {
                    devices_respond_at(ResolvedEndpoint::all_nodes_on(c.scope_id), &c.name)
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
                .map(|(c, _)| (c.name.clone(), ResolvedEndpoint::all_nodes_on(c.scope_id))),
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
    use std::net::{Ipv6Addr, SocketAddrV6};

    use autd3_cpu_wire::udp::{ALL_NODES, PORT};

    use super::*;

    #[test]
    fn the_address_of_the_simulator_given_directly_is_not_a_simulator() {
        assert_eq!(
            resolve(&Interface::Addr(SimulatorAddrs::DEFAULT.data))
                .unwrap()
                .simulator_control,
            None
        );
    }

    #[test]
    fn a_scoped_address_sends_through_its_interface() {
        let scoped = SocketAddrV6::new(Ipv6Addr::from(ALL_NODES), PORT, 0, 7);
        assert_eq!(
            resolve(&Interface::Addr(scoped)).unwrap(),
            ResolvedEndpoint {
                all_devices_dst: scoped,
                scope_id: Some(7),
                simulator_control: None,
            }
        );
    }

    fn interface(name: &str, scope_id: u32) -> (String, ResolvedEndpoint) {
        (name.to_owned(), ResolvedEndpoint::all_nodes_on(scope_id))
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
                resolve(&Interface::Simulator(port)),
                Err(UdpError::InvalidSimulatorPort(p)) if p == port
            ));
        }
    }
}
