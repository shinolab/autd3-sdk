use std::collections::BTreeMap;
use std::net::{Ipv6Addr, SocketAddrV6};

use autd3_cpu_wire::udp::{ALL_NODES, PORT};
use if_addrs::IfAddr;

use super::error::UdpError;

pub(crate) struct LinkLocalInterface {
    pub(crate) name: String,
    pub(crate) scope_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedEndpoint {
    pub(crate) all_devices_dst: SocketAddrV6,
    pub(crate) scope_id: Option<u32>,
}

impl ResolvedEndpoint {
    pub(crate) fn at(addr: SocketAddrV6) -> Self {
        Self {
            all_devices_dst: addr,
            scope_id: (addr.scope_id() != 0).then_some(addr.scope_id()),
        }
    }

    pub(crate) fn all_nodes_on(scope_id: u32) -> Self {
        Self {
            all_devices_dst: SocketAddrV6::new(Ipv6Addr::from(ALL_NODES), PORT, 0, scope_id),
            scope_id: Some(scope_id),
        }
    }
}

pub(crate) fn link_local_interfaces() -> Result<Vec<LinkLocalInterface>, UdpError> {
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
