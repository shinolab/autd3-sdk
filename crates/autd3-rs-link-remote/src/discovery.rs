use std::collections::{BTreeMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6};
use std::time::{Duration, Instant};

use mdns_sd::{
    IfKind, Receiver, ResolvedService, ScopedIp, ServiceDaemon, ServiceEvent, ServiceInfo,
    TryRecvError,
};

use crate::link::RemoteLinkOption;
use crate::wire;

pub const SERVICE_TYPE: &str = "_autd3._tcp.local.";
pub const SIM_SERVICE_TYPE: &str = "_autd3-sim._tcp.local.";
pub const TXT_CONTROL_PORT: &str = "ctrl";
pub const TXT_WIRE_VERSION: &str = "wire";
pub const TXT_SDK_VERSION: &str = "sdk";

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);
const UNREGISTER_TIMEOUT: Duration = Duration::from_millis(500);
const DEFAULT_LINK_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ServerKind {
    #[default]
    Appliance,
    Simulator,
}

impl ServerKind {
    const ALL: [Self; 2] = [Self::Appliance, Self::Simulator];

    #[must_use]
    pub const fn service_type(self) -> &'static str {
        match self {
            Self::Appliance => SERVICE_TYPE,
            Self::Simulator => SIM_SERVICE_TYPE,
        }
    }
}

impl std::fmt::Display for ServerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Appliance => write!(f, "appliance"),
            Self::Simulator => write!(f, "simulator"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Appliance {
    pub instance: String,
    pub host: String,
    pub addr: SocketAddr,
    pub addresses: Vec<SocketAddr>,
    pub control_port: Option<u16>,
    pub wire: Option<u8>,
    pub sdk: Option<String>,
    pub kind: ServerKind,
    pub local: bool,
}

impl std::fmt::Display for Appliance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}", self.instance, self.addr)?;
        match (self.kind, self.local) {
            (ServerKind::Simulator, true) => write!(f, " [local simulator]")?,
            (ServerKind::Simulator, false) => write!(f, " [simulator]")?,
            (ServerKind::Appliance, _) => {}
        }
        if let Some(sdk) = &self.sdk {
            write!(f, " (autd3-sdk {sdk})")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveryOption {
    pub timeout: Duration,
    pub instance: Option<String>,
    pub kind: Option<ServerKind>,
}

impl Default for DiscoveryOption {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            instance: None,
            kind: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DiscoveryError {
    #[error("mDNS error: {0}")]
    Mdns(String),
    #[error(
        "no AUTD3 server answered within {timeout:?}. \
         Check that the appliance is powered up and on the same link or that the simulator is running, \
         or pass its address to `RemoteLinkOption::new`"
    )]
    NotFound { timeout: Duration },
    #[error(
        "{} AUTD3 servers answered with the same priority: {}. \
         Pick one with `DiscoveryOption::instance`, or pass its address to `RemoteLinkOption::new`",
        found.len(),
        found.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "),
    )]
    Ambiguous { found: Vec<Appliance> },
}

fn mdns(err: &mdns_sd::Error) -> DiscoveryError {
    DiscoveryError::Mdns(err.to_string())
}

#[must_use]
pub fn instance_name(fullname: &str) -> String {
    ServerKind::ALL
        .iter()
        .find_map(|kind| fullname.strip_suffix(&format!(".{}", kind.service_type())))
        .unwrap_or(fullname)
        .replace("\\.", ".")
}

fn is_link_local_v6(addr: Ipv6Addr) -> bool {
    addr.segments()[0] & 0xffc0 == 0xfe80
}

fn scope_id(ip: &ScopedIp) -> u32 {
    match ip {
        ScopedIp::V6(v6) => v6.scope_id().index,
        _ => 0,
    }
}

fn socket_addr(ip: &ScopedIp, port: u16) -> Option<SocketAddr> {
    match ip.to_ip_addr() {
        IpAddr::V4(addr) => Some(SocketAddr::new(IpAddr::V4(addr), port)),
        IpAddr::V6(addr) => {
            let scope = scope_id(ip);
            if is_link_local_v6(addr) && scope == 0 {
                return None;
            }
            Some(SocketAddr::V6(SocketAddrV6::new(addr, port, 0, scope)))
        }
    }
}

fn rank(ip: &ScopedIp) -> u8 {
    match ip.to_ip_addr() {
        _ if ip.is_loopback() => 4,
        IpAddr::V4(addr) if addr.is_link_local() => 3,
        IpAddr::V4(_) => 0,
        IpAddr::V6(addr) if is_link_local_v6(addr) => 2,
        IpAddr::V6(_) => 1,
    }
}

fn reachable_addrs(service: &ResolvedService) -> Vec<SocketAddr> {
    let mut candidates: Vec<_> = service
        .addresses
        .iter()
        .filter_map(|ip| socket_addr(ip, service.port).map(|addr| (rank(ip), addr)))
        .collect();
    candidates.sort_by(|(a_rank, a_addr), (b_rank, b_addr)| {
        a_rank
            .cmp(b_rank)
            .then_with(|| a_addr.to_string().cmp(&b_addr.to_string()))
    });
    candidates.into_iter().map(|(_, addr)| addr).collect()
}

fn local_ips() -> HashSet<IpAddr> {
    match if_addrs::get_if_addrs() {
        Ok(interfaces) => interfaces.iter().map(if_addrs::Interface::ip).collect(),
        Err(err) => {
            tracing::warn!(
                %err,
                "cannot list the local interfaces; every discovered server is treated as remote",
            );
            HashSet::new()
        }
    }
}

fn is_local(service: &ResolvedService, local_ips: &HashSet<IpAddr>) -> bool {
    service
        .addresses
        .iter()
        .any(|ip| local_ips.contains(&ip.to_ip_addr()))
}

fn loopback(service: &ResolvedService) -> SocketAddr {
    let advertises_v4 = service.addresses.iter().any(|ip| ip.to_ip_addr().is_ipv4());
    if advertises_v4 {
        SocketAddr::from((Ipv4Addr::LOCALHOST, service.port))
    } else {
        SocketAddr::from((Ipv6Addr::LOCALHOST, service.port))
    }
}

fn appliance(
    service: &ResolvedService,
    kind: ServerKind,
    local_ips: &HashSet<IpAddr>,
) -> Option<Appliance> {
    let instance = instance_name(&service.fullname);
    let local = is_local(service, local_ips);
    let mut addresses = reachable_addrs(service);
    if local {
        let loopback = loopback(service);
        addresses.retain(|addr| *addr != loopback);
        addresses.insert(0, loopback);
    }
    let Some(addr) = addresses.first().copied() else {
        tracing::warn!(
            instance,
            "an AUTD3 server answered but advertises no usable address",
        );
        return None;
    };
    let text = |key: &str| {
        service
            .txt_properties
            .get_property_val_str(key)
            .map(ToOwned::to_owned)
    };
    Some(Appliance {
        instance,
        host: service.host.clone(),
        addr,
        addresses,
        control_port: text(TXT_CONTROL_PORT).and_then(|port| port.parse().ok()),
        wire: text(TXT_WIRE_VERSION).and_then(|version| version.parse().ok()),
        sdk: text(TXT_SDK_VERSION),
        kind,
        local,
    })
}

fn priority(server: &Appliance) -> u8 {
    match (server.kind, server.local) {
        (ServerKind::Simulator, true) => 0,
        (ServerKind::Appliance, _) => 1,
        (ServerKind::Simulator, false) => 2,
    }
}

fn by_priority(mut found: Vec<Appliance>) -> Vec<Appliance> {
    found.sort_by(|a, b| {
        priority(a)
            .cmp(&priority(b))
            .then_with(|| a.instance.cmp(&b.instance))
    });
    found
}

fn select(found: Vec<Appliance>, timeout: Duration) -> Result<Appliance, DiscoveryError> {
    let Some(best) = found.iter().map(priority).min() else {
        return Err(DiscoveryError::NotFound { timeout });
    };
    let mut candidates: Vec<_> = by_priority(found)
        .into_iter()
        .filter(|server| priority(server) == best)
        .collect();
    if candidates.len() == 1 {
        Ok(candidates.remove(0))
    } else {
        Err(DiscoveryError::Ambiguous { found: candidates })
    }
}

fn apply(
    event: ServiceEvent,
    kind: ServerKind,
    local_ips: &HashSet<IpAddr>,
    found: &mut BTreeMap<String, Appliance>,
) {
    match event {
        ServiceEvent::ServiceResolved(service) => {
            if let Some(server) = appliance(&service, kind, local_ips) {
                found.insert(service.fullname.clone(), server);
            }
        }
        ServiceEvent::ServiceRemoved(_, fullname) => {
            found.remove(&fullname);
        }
        _ => {}
    }
}

pub fn discover_all(option: &DiscoveryOption) -> Result<Vec<Appliance>, DiscoveryError> {
    let kinds = option
        .kind
        .map_or_else(|| ServerKind::ALL.to_vec(), |kind| vec![kind]);
    let daemon = ServiceDaemon::new().map_err(|err| mdns(&err))?;
    let receivers: Vec<(ServerKind, Receiver<ServiceEvent>)> = kinds
        .into_iter()
        .map(|kind| daemon.browse(kind.service_type()).map(|rx| (kind, rx)))
        .collect::<Result<_, _>>()
        .map_err(|err| mdns(&err))?;
    let local_ips = local_ips();

    let deadline = Instant::now() + option.timeout;
    let mut found: BTreeMap<String, Appliance> = BTreeMap::new();
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        let mut disconnected = 0;
        for (kind, receiver) in &receivers {
            loop {
                match receiver.try_recv() {
                    Ok(event) => apply(event, *kind, &local_ips, &mut found),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected += 1;
                        break;
                    }
                }
            }
        }
        if disconnected == receivers.len() {
            break;
        }
        std::thread::sleep(remaining.min(POLL_INTERVAL));
    }
    let _ = daemon.shutdown();

    Ok(by_priority(
        found
            .into_values()
            .filter(|found| {
                option
                    .instance
                    .as_ref()
                    .is_none_or(|wanted| *wanted == found.instance)
            })
            .collect(),
    ))
}

pub fn discover(option: &DiscoveryOption) -> Result<Appliance, DiscoveryError> {
    select(discover_all(option)?, option.timeout)
}

fn link_option(appliance: &Appliance) -> RemoteLinkOption {
    RemoteLinkOption {
        addr: appliance.addr,
        timeout: Some(DEFAULT_LINK_TIMEOUT),
    }
}

impl RemoteLinkOption {
    pub fn discover() -> Result<Self, DiscoveryError> {
        Self::discover_with(&DiscoveryOption::default())
    }

    pub fn discover_with(option: &DiscoveryOption) -> Result<Self, DiscoveryError> {
        Ok(link_option(&discover(option)?))
    }

    pub fn discover_appliance() -> Result<Self, DiscoveryError> {
        Self::discover_appliance_with(&DiscoveryOption::default())
    }

    pub fn discover_appliance_with(option: &DiscoveryOption) -> Result<Self, DiscoveryError> {
        Self::discover_with(&with_kind(option, ServerKind::Appliance))
    }

    pub fn discover_simulator() -> Result<Self, DiscoveryError> {
        Self::discover_simulator_with(&DiscoveryOption::default())
    }

    pub fn discover_simulator_with(option: &DiscoveryOption) -> Result<Self, DiscoveryError> {
        Self::discover_with(&with_kind(option, ServerKind::Simulator))
    }
}

fn with_kind(option: &DiscoveryOption, kind: ServerKind) -> DiscoveryOption {
    DiscoveryOption {
        kind: Some(kind),
        ..option.clone()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Advertisement {
    pub instance: String,
    pub port: u16,
    pub control_port: Option<u16>,
    pub exclude_interfaces: Vec<String>,
    pub kind: ServerKind,
}

pub struct AdvertisementHandle {
    daemon: ServiceDaemon,
    fullname: String,
}

impl AdvertisementHandle {
    #[must_use]
    pub fn fullname(&self) -> &str {
        &self.fullname
    }
}

fn service_info(advertisement: &Advertisement) -> Result<ServiceInfo, DiscoveryError> {
    let mut properties = vec![
        (TXT_WIRE_VERSION.to_owned(), wire::VERSION.to_string()),
        (TXT_SDK_VERSION.to_owned(), wire::SDK_VERSION.to_owned()),
    ];
    if let Some(port) = advertisement.control_port {
        properties.push((TXT_CONTROL_PORT.to_owned(), port.to_string()));
    }

    Ok(ServiceInfo::new(
        advertisement.kind.service_type(),
        &advertisement.instance,
        &format!("{}.local.", advertisement.instance),
        "",
        advertisement.port,
        &properties[..],
    )
    .map_err(|err| mdns(&err))?
    .enable_addr_auto())
}

pub fn advertise(advertisement: &Advertisement) -> Result<AdvertisementHandle, DiscoveryError> {
    let info = service_info(advertisement)?;
    let fullname = info.get_fullname().to_owned();

    let daemon = ServiceDaemon::new().map_err(|err| mdns(&err))?;
    for interface in &advertisement.exclude_interfaces {
        daemon
            .disable_interface(IfKind::Name(interface.clone()))
            .map_err(|err| mdns(&err))?;
    }
    daemon.register(info).map_err(|err| mdns(&err))?;

    Ok(AdvertisementHandle { daemon, fullname })
}

impl Drop for AdvertisementHandle {
    fn drop(&mut self) {
        if let Ok(status) = self.daemon.unregister(&self.fullname) {
            let _ = status.recv_timeout(UNREGISTER_TIMEOUT);
        }
        let _ = self.daemon.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use mdns_sd::{InterfaceId, ScopedIpV4};

    use super::*;

    #[test]
    fn the_default_timeout_outlives_a_full_session_open() {
        assert!(DEFAULT_LINK_TIMEOUT >= crate::server::SESSION_OPEN_TIMEOUT * 2);
    }

    #[test]
    fn the_instance_name_drops_the_service_type() {
        assert_eq!(
            instance_name(&format!("autd3-0a1b2c3d.{SERVICE_TYPE}")),
            "autd3-0a1b2c3d",
        );
        assert_eq!(
            instance_name(&format!("autd3-sim-lab-pc-8080.{SIM_SERVICE_TYPE}")),
            "autd3-sim-lab-pc-8080",
        );
        assert_eq!(instance_name("autd3-0a1b2c3d"), "autd3-0a1b2c3d");
    }

    #[test]
    fn a_routable_address_wins_over_a_link_local_one() {
        let intf = InterfaceId {
            name: "eth0".to_owned(),
            index: 2,
        };
        let routable = ScopedIp::V4(ScopedIpV4::new(Ipv4Addr::new(192, 168, 1, 5), intf.clone()));
        let link_local = ScopedIp::V4(ScopedIpV4::new(Ipv4Addr::new(169, 254, 1, 5), intf));
        assert!(rank(&routable) < rank(&link_local));
        assert!(rank(&link_local) < rank(&ScopedIp::from(IpAddr::V4(Ipv4Addr::LOCALHOST))));
    }

    #[test]
    fn a_link_local_ipv6_address_wins_over_a_link_local_ipv4_one() {
        let intf = InterfaceId {
            name: "eth0".to_owned(),
            index: 2,
        };
        let v6 = ScopedIp::from(IpAddr::V6("fe80::1".parse().unwrap()));
        let v4 = ScopedIp::V4(ScopedIpV4::new(Ipv4Addr::new(169, 254, 1, 5), intf));
        assert!(
            rank(&v6) < rank(&v4),
            "a directly wired Linux host answers on its own IPv6 link-local address, \
             but reaches 169.254.0.0/16 only with an explicit connection profile",
        );
        assert!(
            rank(&ScopedIp::from(IpAddr::V6("2001:db8::1".parse().unwrap()))) < rank(&v6),
            "a routable address still wins over any link-local one",
        );
    }

    #[test]
    fn a_discovered_endpoint_carries_a_timeout_so_an_unreachable_answer_cannot_hang() {
        let found = remote(&resolved("169.254.1.5")).unwrap();
        assert_eq!(link_option(&found).timeout, Some(DEFAULT_LINK_TIMEOUT));
        assert!(
            RemoteLinkOption::new(found.addr).timeout.is_none(),
            "an address the caller typed in keeps the OS defaults",
        );
    }

    #[test]
    fn a_scopeless_link_local_ipv6_address_is_unusable() {
        let scopeless = ScopedIp::from(IpAddr::V6("fe80::1".parse().unwrap()));
        assert!(socket_addr(&scopeless, 8080).is_none());

        let global = ScopedIp::from(IpAddr::V6("2001:db8::1".parse().unwrap()));
        assert!(socket_addr(&global, 8080).is_some());
    }

    #[test]
    fn the_advertisement_carries_the_endpoint_and_the_versions() {
        let info = service_info(&Advertisement {
            instance: "autd3-0a1b2c3d".to_owned(),
            port: 8080,
            control_port: Some(8081),
            ..Advertisement::default()
        })
        .unwrap();

        assert_eq!(
            info.get_fullname(),
            format!("autd3-0a1b2c3d.{SERVICE_TYPE}")
        );
        assert_eq!(info.get_hostname(), "autd3-0a1b2c3d.local.");
        assert_eq!(info.get_port(), 8080);
        assert!(info.is_addr_auto());
        assert_eq!(
            info.get_property_val_str(TXT_WIRE_VERSION),
            Some(wire::VERSION.to_string().as_str()),
        );
        assert_eq!(
            info.get_property_val_str(TXT_SDK_VERSION),
            Some(wire::SDK_VERSION),
        );
        assert_eq!(info.get_property_val_str(TXT_CONTROL_PORT), Some("8081"));
    }

    #[test]
    fn a_server_without_a_control_api_advertises_no_control_port() {
        let info = service_info(&Advertisement {
            instance: "autd3-0a1b2c3d".to_owned(),
            port: 8080,
            ..Advertisement::default()
        })
        .unwrap();
        assert_eq!(info.get_property_val_str(TXT_CONTROL_PORT), None);
    }

    #[test]
    fn a_simulator_advertises_under_its_own_service_type() {
        let info = service_info(&Advertisement {
            instance: "autd3-sim-lab-pc-8080".to_owned(),
            port: 8080,
            kind: ServerKind::Simulator,
            ..Advertisement::default()
        })
        .unwrap();
        assert_eq!(info.get_type(), SIM_SERVICE_TYPE);
        assert_eq!(
            info.get_fullname(),
            format!("autd3-sim-lab-pc-8080.{SIM_SERVICE_TYPE}"),
            "a client that only browses {SERVICE_TYPE} never sees a simulator",
        );
        assert_eq!(info.get_property_val_str(TXT_CONTROL_PORT), None);
    }

    fn resolved(addresses: &str) -> ResolvedService {
        ServiceInfo::new(
            SERVICE_TYPE,
            "autd3-0a1b2c3d",
            "autd3-0a1b2c3d.local.",
            addresses,
            8080,
            &[("wire", "6"), ("sdk", "0.4.0"), ("ctrl", "8081")][..],
        )
        .unwrap()
        .as_resolved_service()
    }

    fn remote(service: &ResolvedService) -> Option<Appliance> {
        appliance(service, ServerKind::Appliance, &HashSet::new())
    }

    fn ips(addresses: &[&str]) -> HashSet<IpAddr> {
        addresses.iter().map(|ip| ip.parse().unwrap()).collect()
    }

    #[test]
    fn an_answer_becomes_an_endpoint_plus_the_versions_behind_it() {
        let appliance = remote(&resolved("169.254.1.5")).unwrap();
        assert_eq!(appliance.instance, "autd3-0a1b2c3d");
        assert_eq!(appliance.host, "autd3-0a1b2c3d.local.");
        assert_eq!(appliance.addr, "169.254.1.5:8080".parse().unwrap());
        assert_eq!(appliance.wire, Some(6));
        assert_eq!(appliance.sdk.as_deref(), Some("0.4.0"));
        assert_eq!(appliance.control_port, Some(8081));
        assert_eq!(appliance.kind, ServerKind::Appliance);
        assert!(!appliance.local);
    }

    #[test]
    fn the_routable_answer_is_the_one_we_connect_to() {
        let appliance = remote(&resolved("169.254.1.5,192.168.1.5,127.0.0.1")).unwrap();
        assert_eq!(appliance.addr, "192.168.1.5:8080".parse().unwrap());
        assert_eq!(
            appliance.addresses,
            [
                "192.168.1.5:8080".parse().unwrap(),
                "169.254.1.5:8080".parse().unwrap(),
                "127.0.0.1:8080".parse().unwrap(),
            ],
            "every answer stays available so a caller can override the pick",
        );
    }

    #[test]
    fn an_appliance_without_a_usable_address_is_dropped() {
        assert!(remote(&resolved("fe80::1")).is_none());
    }

    #[test]
    fn a_server_answering_with_one_of_our_addresses_is_local_and_reached_over_loopback() {
        let server = appliance(
            &resolved("192.168.1.5,169.254.1.5"),
            ServerKind::Simulator,
            &ips(&["127.0.0.1", "192.168.1.5"]),
        )
        .unwrap();
        assert!(server.local);
        assert_eq!(server.addr, "127.0.0.1:8080".parse().unwrap());
        assert_eq!(
            server.addresses,
            [
                "127.0.0.1:8080".parse().unwrap(),
                "192.168.1.5:8080".parse().unwrap(),
                "169.254.1.5:8080".parse().unwrap(),
            ],
            "loopback survives a VPN or Wi-Fi switch, and the advertised addresses stay as fallbacks",
        );
    }

    #[test]
    fn a_server_sharing_none_of_our_addresses_is_remote() {
        let server = appliance(
            &resolved("192.168.1.5"),
            ServerKind::Simulator,
            &ips(&["127.0.0.1", "192.168.1.7", "172.17.0.1"]),
        )
        .unwrap();
        assert!(!server.local);
        assert_eq!(server.addr, "192.168.1.5:8080".parse().unwrap());
    }

    #[test]
    fn a_local_server_advertising_only_ipv6_is_reached_over_ipv6_loopback() {
        let server = appliance(
            &resolved("2001:db8::5"),
            ServerKind::Simulator,
            &ips(&["::1", "2001:db8::5"]),
        )
        .unwrap();
        assert_eq!(server.addr, "[::1]:8080".parse().unwrap());
    }

    #[test]
    fn a_local_server_with_only_a_scopeless_link_local_answer_is_still_reachable() {
        let server = appliance(
            &resolved("fe80::1"),
            ServerKind::Simulator,
            &ips(&["fe80::1"]),
        )
        .unwrap();
        assert_eq!(server.addr, "[::1]:8080".parse().unwrap());
    }

    fn server(instance: &str, kind: ServerKind, local: bool) -> Appliance {
        Appliance {
            instance: instance.to_owned(),
            host: format!("{instance}.local."),
            addr: "192.168.1.5:8080".parse().unwrap(),
            addresses: Vec::new(),
            control_port: None,
            wire: None,
            sdk: None,
            kind,
            local,
        }
    }

    fn pick(found: Vec<Appliance>) -> Result<Appliance, DiscoveryError> {
        select(found, DEFAULT_TIMEOUT)
    }

    #[test]
    fn a_local_simulator_wins_over_everything() {
        let picked = pick(vec![
            server("autd3-0a1b2c3d", ServerKind::Appliance, false),
            server("autd3-sim-other-8080", ServerKind::Simulator, false),
            server("autd3-sim-mine-8080", ServerKind::Simulator, true),
        ])
        .unwrap();
        assert_eq!(picked.instance, "autd3-sim-mine-8080");
    }

    #[test]
    fn an_appliance_wins_over_a_simulator_on_another_host() {
        let picked = pick(vec![
            server("autd3-sim-other-8080", ServerKind::Simulator, false),
            server("autd3-0a1b2c3d", ServerKind::Appliance, false),
        ])
        .unwrap();
        assert_eq!(picked.instance, "autd3-0a1b2c3d");
    }

    #[test]
    fn a_simulator_on_another_host_is_the_last_resort() {
        let picked = pick(vec![server(
            "autd3-sim-other-8080",
            ServerKind::Simulator,
            false,
        )])
        .unwrap();
        assert_eq!(picked.instance, "autd3-sim-other-8080");
    }

    #[test]
    fn two_servers_at_the_top_priority_are_ambiguous_whatever_sits_below() {
        let Err(DiscoveryError::Ambiguous { found }) = pick(vec![
            server("autd3-0a1b2c3d", ServerKind::Appliance, false),
            server("autd3-sim-mine-8081", ServerKind::Simulator, true),
            server("autd3-sim-mine-8080", ServerKind::Simulator, true),
        ]) else {
            panic!("two local simulators must not be resolved by picking one");
        };
        let names: Vec<_> = found.iter().map(|s| s.instance.as_str()).collect();
        assert_eq!(names, ["autd3-sim-mine-8080", "autd3-sim-mine-8081"]);
    }

    #[test]
    fn a_kind_specific_entry_point_overrides_the_kind_and_keeps_the_rest() {
        let option = DiscoveryOption {
            timeout: Duration::from_millis(500),
            instance: Some("autd3-sim-lab-pc-8080".to_owned()),
            kind: Some(ServerKind::Appliance),
        };
        let simulator = with_kind(&option, ServerKind::Simulator);
        assert_eq!(simulator.kind, Some(ServerKind::Simulator));
        assert_eq!(simulator.timeout, option.timeout);
        assert_eq!(simulator.instance, option.instance);
    }

    #[test]
    fn nothing_found_is_not_found() {
        assert!(matches!(
            pick(Vec::new()),
            Err(DiscoveryError::NotFound { .. })
        ));
    }

    #[test]
    fn the_full_listing_is_in_priority_order() {
        let listed = by_priority(vec![
            server("autd3-sim-other-8080", ServerKind::Simulator, false),
            server("autd3-b", ServerKind::Appliance, false),
            server("autd3-a", ServerKind::Appliance, false),
            server("autd3-sim-mine-8080", ServerKind::Simulator, true),
        ]);
        let names: Vec<_> = listed.iter().map(|s| s.instance.as_str()).collect();
        assert_eq!(
            names,
            [
                "autd3-sim-mine-8080",
                "autd3-a",
                "autd3-b",
                "autd3-sim-other-8080"
            ],
        );
    }
}
