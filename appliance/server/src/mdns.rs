use autd3_rs_link_remote::{Advertisement, AdvertisementHandle};

use crate::config::Config;

const CPUINFO_PATH: &str = "/proc/cpuinfo";
const MACHINE_ID_PATH: &str = "/etc/machine-id";
const INSTANCE_PREFIX: &str = "autd3";
const SERIAL_DIGITS: usize = 8;

fn tail(id: &str) -> Option<String> {
    let id: String = id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    (id.len() >= SERIAL_DIGITS).then(|| id[id.len() - SERIAL_DIGITS..].to_owned())
}

fn board_serial() -> Option<String> {
    let cpuinfo = std::fs::read_to_string(CPUINFO_PATH).ok()?;
    cpuinfo
        .lines()
        .find_map(|line| line.strip_prefix("Serial"))
        .and_then(|line| line.split(':').nth(1))
        .and_then(|serial| tail(serial.trim()))
}

fn machine_id() -> Option<String> {
    tail(std::fs::read_to_string(MACHINE_ID_PATH).ok()?.trim())
}

#[must_use]
pub fn default_instance() -> String {
    board_serial().or_else(machine_id).map_or_else(
        || INSTANCE_PREFIX.to_owned(),
        |id| format!("{INSTANCE_PREFIX}-{id}"),
    )
}

#[must_use]
pub fn instance(config: &Config) -> String {
    config
        .mdns
        .instance
        .clone()
        .unwrap_or_else(default_instance)
}

#[must_use]
pub fn advertisement(config: &Config) -> Option<Advertisement> {
    config.mdns.enabled.then(|| Advertisement {
        instance: instance(config),
        port: config.server.bind.port(),
        control_port: config.control.enabled.then(|| config.control.bind.port()),
        exclude_interfaces: config.bus.interface.clone().into_iter().collect(),
    })
}

#[must_use]
pub fn advertise(config: &Config) -> Option<AdvertisementHandle> {
    let advertisement = advertisement(config)?;
    match autd3_rs_link_remote::advertise(&advertisement) {
        Ok(handle) => {
            tracing::info!(
                service = handle.fullname(),
                port = advertisement.port,
                "advertising the appliance over mDNS",
            );
            Some(handle)
        }
        Err(err) => {
            tracing::warn!(
                %err,
                "failed to advertise the appliance over mDNS; \
                 clients have to be given the address explicitly",
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identifier_becomes_the_last_eight_alphanumerics() {
        assert_eq!(tail("1000000099C06885").as_deref(), Some("99c06885"));
        assert_eq!(
            tail("812affc8522241d7b186fe440fb37efd").as_deref(),
            Some("0fb37efd"),
        );
        assert_eq!(tail("short").as_deref(), None);
    }

    #[test]
    fn the_ethercat_interface_is_kept_out_of_the_advertisement() {
        let config: Config = toml::from_str(
            r#"
            [bus]
            interface = "eth0"
            "#,
        )
        .unwrap();
        let advertisement = advertisement(&config).unwrap();
        assert_eq!(advertisement.exclude_interfaces, ["eth0"]);
        assert_eq!(advertisement.port, config.server.bind.port());
        assert_eq!(
            advertisement.control_port,
            Some(config.control.bind.port()),
            "a client that finds the appliance can go straight to its control API",
        );
        assert!(advertisement.instance.starts_with(INSTANCE_PREFIX));
    }

    #[test]
    fn the_config_can_name_the_instance_and_turn_the_advertisement_off() {
        let named: Config = toml::from_str(
            r#"
            [bus]
            interface = "eth0"
            [mdns]
            instance = "autd3-lab-1"
            "#,
        )
        .unwrap();
        assert_eq!(advertisement(&named).unwrap().instance, "autd3-lab-1");

        let off: Config = toml::from_str(
            r#"
            [bus]
            interface = "eth0"
            [mdns]
            enabled = false
            "#,
        )
        .unwrap();
        assert!(advertisement(&off).is_none());
    }
}
