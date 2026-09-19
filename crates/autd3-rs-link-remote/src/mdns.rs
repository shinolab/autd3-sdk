use std::time::Duration;

use crate::client::Appliance;

pub const SERVICE_TYPE: &str = "_autd3._tcp.local.";
pub const SIM_SERVICE_TYPE: &str = "_autd3-sim._tcp.local.";
pub const TXT_CONTROL_PORT: &str = "ctrl";
pub const TXT_WIRE_VERSION: &str = "wire";
pub const TXT_SDK_VERSION: &str = "sdk";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ServerKind {
    #[default]
    Appliance,
    Simulator,
}

impl ServerKind {
    pub(crate) const ALL: [Self; 2] = [Self::Appliance, Self::Simulator];

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

pub(crate) fn mdns(err: &mdns_sd::Error) -> DiscoveryError {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
