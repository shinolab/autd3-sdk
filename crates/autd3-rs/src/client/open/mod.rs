mod enumerate;
mod iface_probe;
mod requester;

use std::time::Instant;

use autd3_rs_core::geometry::Geometry;
use autd3_rs_core::{DeviceClock, MAX_DEVICES};

use crate::udp::{Channel, TransportOption, UdpError};

use enumerate::{ExpectedCount, SyncOnOpen, bring_up};

pub fn discover(option: &TransportOption) -> Result<usize, UdpError> {
    open_with(
        option,
        ExpectedCount::All,
        SyncOnOpen::Skip,
        &DeviceClock::new(),
    )
    .map(|channel| channel.num_devices())
}

pub(crate) fn open_channel(
    option: &TransportOption,
    geometry: &Geometry,
    device_clock: &DeviceClock,
) -> Result<Channel, UdpError> {
    open_with(
        option,
        ExpectedCount::Exactly(geometry.num_devices()),
        if option.sync_on_open {
            SyncOnOpen::Synchronize
        } else {
            SyncOnOpen::Skip
        },
        device_clock,
    )
}

fn open_with(
    option: &TransportOption,
    expected: ExpectedCount,
    sync_on_open: SyncOnOpen,
    device_clock: &DeviceClock,
) -> Result<Channel, UdpError> {
    option.validate()?;
    let deadline = Instant::now().checked_add(option.open_timeout);
    if let ExpectedCount::Exactly(num_devices) = expected
        && !(1..=MAX_DEVICES).contains(&num_devices)
    {
        return Err(UdpError::InvalidDeviceCount(num_devices));
    }
    let target = iface_probe::resolve(&option.iface)?;
    let mut channel = Channel::open(
        target.all_devices_dst,
        target.scope_id,
        option.send_rate_limit_percent,
        option.send_buffer_bytes,
    )?;
    let device_addrs = bring_up(
        &channel,
        option,
        deadline,
        expected,
        sync_on_open,
        device_clock,
    )?;
    channel.attach_devices(device_addrs);
    Ok(channel)
}
