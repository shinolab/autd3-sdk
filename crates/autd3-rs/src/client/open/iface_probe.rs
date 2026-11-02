use autd3_cpu_wire::udp::Kind;
use autd3_rs_core::Interface;

use crate::udp::{Channel, ResolvedEndpoint, UdpError, link_local_interfaces};

use super::enumerate::{REQUEST_REPLY_WAIT, collect_responses};

pub(super) async fn resolve(iface: &Interface) -> Result<ResolvedEndpoint, UdpError> {
    async fn devices_respond_at(endpoint: ResolvedEndpoint) -> Result<bool, UdpError> {
        let channel = Channel::open(endpoint.all_devices_dst, endpoint.scope_id, None).await?;
        channel
            .send(
                endpoint.all_devices_dst,
                Kind::ReadUnitInfo,
                0,
                [&[], &[]],
                None,
            )
            .await?;
        match collect_responses(&channel, Kind::ReadUnitInfo, 0, REQUEST_REPLY_WAIT, Some(1)).await
        {
            Ok(responses) => Ok(!responses.is_empty()),
            Err(UdpError::UnsupportedVersion { .. }) => Ok(true),
            Err(e) => Err(e),
        }
    }

    match iface {
        Interface::Addr(addr) => Ok(ResolvedEndpoint::at(*addr)),
        Interface::Name(name) => link_local_interfaces()?
            .iter()
            .find(|c| c.name == *name)
            .map(|c| ResolvedEndpoint::all_nodes_on(c.scope_id))
            .ok_or_else(|| UdpError::InterfaceNotFound(name.clone())),
        Interface::Auto => {
            for candidate in link_local_interfaces()? {
                let endpoint = ResolvedEndpoint::all_nodes_on(candidate.scope_id);
                match devices_respond_at(endpoint).await {
                    Ok(true) => {
                        tracing::info!(interface = %candidate.name, "found AUTD3 devices");
                        return Ok(endpoint);
                    }
                    Ok(false) => {}
                    Err(e) => {
                        tracing::debug!(interface = %candidate.name, "asking an interface for AUTD3 devices failed: {e}");
                    }
                }
            }
            Err(UdpError::NoInterfaceFound)
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv6Addr, SocketAddrV6};

    use autd3_cpu_wire::udp::{ALL_NODES, PORT};

    use super::*;

    #[tokio::test]
    async fn a_scoped_address_sends_through_its_interface() {
        let scoped = SocketAddrV6::new(Ipv6Addr::from(ALL_NODES), PORT, 0, 7);
        assert_eq!(
            resolve(&Interface::Addr(scoped)).await.unwrap(),
            ResolvedEndpoint {
                all_devices_dst: scoped,
                scope_id: Some(7),
            }
        );
    }
}
