use std::net::SocketAddrV6;
use std::time::Duration;

use autd3_cpu_wire::udp::{Kind, UnitInfo};
use zerocopy::FromBytes;

use super::bus::UdpBus;
use super::channel::Channel;
use super::enumerate::same_endpoint;
use super::error::UdpError;

pub struct Monitor {
    channel: Channel,
    units: Vec<SocketAddrV6>,
    response_timeout: Duration,
}

impl Monitor {
    pub fn new(bus: &UdpBus) -> Result<Self, UdpError> {
        let group = bus.channel().group();
        let scope = (group.scope_id() != 0).then_some(group.scope_id());
        Ok(Self {
            channel: Channel::open(group, scope, None, None)?,
            units: bus.units().to_vec(),
            response_timeout: bus.response_timeout(),
        })
    }

    pub fn read(&self) -> Result<Vec<Option<UnitInfo>>, UdpError> {
        let group = self.channel.group();
        let msg_id = self.channel.request(group, Kind::ReadUnitInfo, &[])?;
        let mut infos = vec![None; self.units.len()];
        for response in self.channel.collect(
            Kind::ReadUnitInfo,
            msg_id,
            self.response_timeout,
            Some(self.units.len()),
        )? {
            let Some(device) = self
                .units
                .iter()
                .position(|&unit| same_endpoint(unit, response.src))
            else {
                continue;
            };
            infos[device] = UnitInfo::read_from_prefix(&response.body)
                .ok()
                .map(|(info, _)| info);
        }
        Ok(infos)
    }
}
