use ethercrab::{MainDeviceConfig, Timeouts, std::ethercat_now};
use tokio::runtime::Handle;

use crate::error::EtherCrabLinkError;
use crate::link::{DETECT_PDI_LEN, MAX_SUBDEVICES, SUBDEVICE_NAME};
use crate::transport::Transport;

pub async fn lookup_autd() -> Result<String, EtherCrabLinkError> {
    let handle = Handle::try_current().map_err(|_| EtherCrabLinkError::NoTokioRuntime)?;
    let devices = pcap::Device::list()?;

    tracing::debug!("found {} network interfaces", devices.len());
    for interface in devices {
        tracing::debug!(
            "searching AUTD devices on {} ({})",
            interface.name,
            interface.desc.as_deref().unwrap_or("no description")
        );

        if interface.flags.is_loopback() {
            tracing::debug!("skipping loopback interface: {}", interface.name);
            continue;
        }
        if interface.flags.is_wireless() {
            tracing::debug!("skipping wireless interface: {}", interface.name);
            continue;
        }

        let Ok(transport) = Transport::open(
            &handle,
            &interface.name,
            Timeouts::default(),
            MainDeviceConfig::default(),
        ) else {
            tracing::trace!("failed to open transport on {}", interface.name);
            continue;
        };

        let found = match Box::pin(
            transport
                .maindevice()
                .init_single_group::<MAX_SUBDEVICES, DETECT_PDI_LEN>(ethercat_now),
        )
        .await
        {
            Ok(group) => {
                tracing::trace!(
                    "found {} EtherCAT subdevice(s) on {}",
                    group.len(),
                    interface.name
                );
                !group.is_empty()
                    && group
                        .iter(transport.maindevice())
                        .all(|sub_device| sub_device.name() == SUBDEVICE_NAME)
            }
            Err(e) => {
                tracing::trace!("failed to initialize EtherCAT on {}: {e}", interface.name);
                false
            }
        };

        drop(transport);

        if found {
            return Ok(interface.name);
        }
    }

    Err(EtherCrabLinkError::DeviceNotFound)
}
