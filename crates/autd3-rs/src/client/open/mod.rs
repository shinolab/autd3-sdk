mod enumerate;
mod iface_probe;
mod requester;

use std::time::Instant;

use autd3_rs_core::{DeviceClock, MAX_DEVICES};

use crate::udp::{Channel, TransportOption, UdpError};

use enumerate::bring_up;

pub(crate) fn open(
    option: &TransportOption,
    num_devices: usize,
    device_clock: &DeviceClock,
) -> Result<Channel, UdpError> {
    option.validate()?;
    let deadline = Instant::now().checked_add(option.open_timeout);
    if !(1..=MAX_DEVICES).contains(&num_devices) {
        return Err(UdpError::InvalidDeviceCount(num_devices));
    }
    let target = iface_probe::resolve(&option.iface)?;
    let mut channel = Channel::open(
        target.all_devices_dst,
        target.scope_id,
        option.send_rate_limit_percent,
        option.send_buffer_bytes,
    )?;
    let device_addrs = bring_up(&channel, option, deadline, num_devices, device_clock)?;
    channel.attach_devices(device_addrs);
    Ok(channel)
}
