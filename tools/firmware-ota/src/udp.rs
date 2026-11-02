use std::time::Duration;

use autd3_rs::{Error, Interface, TransportOption, UdpError};

use crate::update::{ACK_TIMEOUT, KeepaliveClient};

#[derive(Debug, thiserror::Error)]
#[error("{0}{hint}", hint = hint(.0))]
pub(crate) struct OpenError(#[from] Error);

fn option(interface: Option<&str>, open_timeout: Option<Duration>) -> TransportOption {
    let default = TransportOption::default();
    TransportOption {
        iface: interface.map_or(Interface::Auto, |name| Interface::Name(name.to_owned())),
        open_timeout: open_timeout.unwrap_or(default.open_timeout),
        ack_timeout: ACK_TIMEOUT,
        sync_on_open: false,
        ..default
    }
}

pub(crate) async fn connect(
    interface: Option<&str>,
    open_timeout: Option<Duration>,
    num_devices: usize,
) -> Result<KeepaliveClient, OpenError> {
    Ok(KeepaliveClient::open(&option(interface, open_timeout), num_devices).await?)
}

fn udp_cause(e: &Error) -> Option<&UdpError> {
    if let Error::Network(cause) = e {
        Some(cause)
    } else {
        None
    }
}

fn hint(e: &Error) -> String {
    let found_nothing = udp_cause(e).is_some_and(|e| {
        matches!(
            e,
            UdpError::NoInterfaceFound
                | UdpError::InterfaceNotFound(_)
                | UdpError::EnumerationTimeout { found: 0, .. }
        )
    });
    if found_nothing {
        let port = autd3_cpu_wire::udp::PORT;
        format!(
            "; if the devices are connected and powered, the host firewall may be dropping their \
             replies: allow inbound UDP from fe80::/10 source port {port} on that interface \
             (e.g. `sudo ufw allow in on <interface> proto udp from fe80::/10 port {port}`; \
             on Windows, an inbound rule for UDP from remote port {port})"
        )
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_firmware_emulator::udp::UdpEmulator;

    use super::*;

    #[test]
    fn no_arguments_mean_the_library_defaults() {
        assert_eq!(
            option(None, None),
            TransportOption {
                ack_timeout: ACK_TIMEOUT,
                sync_on_open: false,
                ..TransportOption::default()
            }
        );
    }

    #[test]
    fn interface_and_open_timeout_reach_the_option() {
        assert_eq!(
            option(Some("eth1"), Some(Duration::from_secs(20))),
            TransportOption {
                iface: Interface::Name("eth1".to_string()),
                open_timeout: Duration::from_secs(20),
                ack_timeout: ACK_TIMEOUT,
                sync_on_open: false,
                ..TransportOption::default()
            }
        );
    }

    #[tokio::test]
    #[ignore = "depends on real time"]
    async fn a_given_device_count_must_match_the_chain() {
        let emulator = UdpEmulator::spawn(3).unwrap();
        let option = TransportOption {
            iface: emulator.interface(),
            open_timeout: Duration::from_secs(6),
            sync_on_open: false,
            ..TransportOption::default()
        };
        let err = OpenError::from(KeepaliveClient::open(&option, 2).await.err().unwrap());
        assert!(
            matches!(
                udp_cause(&err.0),
                Some(UdpError::DeviceCountMismatch {
                    expected: 2,
                    found: 3
                })
            ),
            "{err}"
        );
    }

    #[test]
    fn only_a_search_that_found_nothing_suggests_the_firewall() {
        for nothing in [
            UdpError::NoInterfaceFound,
            UdpError::InterfaceNotFound("eth0".to_owned()),
            UdpError::EnumerationTimeout {
                expected: 1,
                found: 0,
            },
        ] {
            let message = OpenError(nothing.into()).to_string();
            assert!(
                message.contains("firewall") && message.contains("44336"),
                "{message}"
            );
        }

        let message = OpenError(
            UdpError::DeviceCountMismatch {
                expected: 2,
                found: 3,
            }
            .into(),
        )
        .to_string();
        assert!(!message.contains("firewall"), "{message}");
    }
}
