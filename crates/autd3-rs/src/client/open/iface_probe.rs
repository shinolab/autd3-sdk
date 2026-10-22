use autd3_cpu_wire::udp::Kind;
use autd3_rs_core::Interface;

use crate::udp::{Channel, ResolvedEndpoint, UdpError, link_local_interfaces};

use super::enumerate::{REQUEST_REPLY_WAIT, collect_responses};

pub(super) fn resolve(iface: &Interface) -> Result<ResolvedEndpoint, UdpError> {
    match iface {
        Interface::Addr(addr) => Ok(ResolvedEndpoint::at(*addr)),
        Interface::Name(name) => link_local_interfaces()?
            .iter()
            .find(|c| c.name == *name)
            .map(|c| ResolvedEndpoint::all_nodes_on(c.scope_id))
            .ok_or_else(|| UdpError::InterfaceNotFound(name.clone())),
        Interface::Auto => resolve_auto(),
        _ => unreachable!(),
    }
}

fn resolve_auto() -> Result<ResolvedEndpoint, UdpError> {
    fn devices_respond_at(endpoint: ResolvedEndpoint) -> Result<bool, UdpError> {
        let channel = Channel::open(endpoint.all_devices_dst, endpoint.scope_id, None)?;
        channel.send(
            endpoint.all_devices_dst,
            Kind::ReadUnitInfo,
            0,
            [&[], &[]],
            None,
        )?;
        match collect_responses(&channel, Kind::ReadUnitInfo, 0, REQUEST_REPLY_WAIT, Some(1)) {
            Ok(responses) => Ok(!responses.is_empty()),
            Err(UdpError::UnsupportedVersion { .. }) => Ok(true),
            Err(e) => Err(e),
        }
    }

    let found = link_local_interfaces()?
        .into_iter()
        .find(|c| {
            devices_respond_at(ResolvedEndpoint::all_nodes_on(c.scope_id)).unwrap_or_else(|e| {
                tracing::debug!(interface = %c.name, "asking an interface for AUTD3 devices failed: {e}");
                false
            })
        })
        .ok_or(UdpError::NoInterfaceFound)?;
    tracing::info!(interface = %found.name, "found AUTD3 devices");
    Ok(ResolvedEndpoint::all_nodes_on(found.scope_id))
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv6Addr, SocketAddrV6};

    use autd3_cpu_wire::udp::{ALL_NODES, PORT};

    use super::*;

    #[test]
    fn a_scoped_address_sends_through_its_interface() {
        let scoped = SocketAddrV6::new(Ipv6Addr::from(ALL_NODES), PORT, 0, 7);
        assert_eq!(
            resolve(&Interface::Addr(scoped)).unwrap(),
            ResolvedEndpoint {
                all_devices_dst: scoped,
                scope_id: Some(7),
            }
        );
    }
}
