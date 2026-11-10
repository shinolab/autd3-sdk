mod enumerate;
mod iface_probe;

use autd3_rs_core::MAX_DEVICES;

use crate::udp::{Channel, TransportOption, UdpError};

use super::device_clock::DeviceClock;

use enumerate::bring_up;

pub(crate) async fn open(
    option: &TransportOption,
    num_devices: usize,
    device_clock: &mut DeviceClock,
) -> Result<Channel, UdpError> {
    if !(1..=MAX_DEVICES).contains(&num_devices) {
        return Err(UdpError::InvalidDeviceCount(num_devices));
    }
    let target = iface_probe::resolve(&option.iface).await?;
    let mut channel = Channel::open(
        target.all_devices_dst,
        target.scope_id,
        option.send_buffer_bytes,
    )
    .await?;
    let device_addrs = bring_up(&channel, option, num_devices, device_clock).await?;
    channel.attach_devices(device_addrs);
    Ok(channel)
}
