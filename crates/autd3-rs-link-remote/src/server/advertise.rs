use std::time::Duration;

use mdns_sd::{IfKind, ServiceDaemon, ServiceInfo};

use crate::mdns::{
    DiscoveryError, ServerKind, TXT_CONTROL_PORT, TXT_SDK_VERSION, TXT_WIRE_VERSION, mdns,
};
use crate::wire;

const UNREGISTER_TIMEOUT: Duration = Duration::from_millis(500);

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
    use crate::mdns::{SERVICE_TYPE, SIM_SERVICE_TYPE};

    use super::*;

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
}
