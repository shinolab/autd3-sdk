use std::net::SocketAddrV6;
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::{
    AssignIdBody, Flags, Kind, Role, SetTimeBody, Status, UnblockReply, UnitInfo,
};
use autd3_rs_core::DeviceClock;
use autd3_rs_core::value::SysTime;
use zerocopy::little_endian::U64;
use zerocopy::{FromBytes, IntoBytes};

use super::channel::{Channel, Response};
use super::error::UdpError;
use super::option::TransportOption;
use super::state::SYNCED;

const UNICAST_ATTEMPTS: usize = 3;
const RESET_SETTLE: Duration = Duration::from_millis(100);
const DISCOVER_LINGER: Duration = Duration::from_millis(10);
const DISCOVER_RETRY_INTERVAL: Duration = Duration::from_millis(100);
const SYNC_POLL_INTERVAL: Duration = Duration::from_millis(50);
const GRANDMASTER: u8 = 0;
const DEVICE_TIME_AT_OPEN: SysTime = SysTime::ZERO;

pub(crate) fn same_endpoint(a: SocketAddrV6, b: SocketAddrV6) -> bool {
    a.ip() == b.ip() && a.port() == b.port()
}

fn unit_info(response: &Response) -> Option<UnitInfo> {
    UnitInfo::read_from_prefix(&response.body)
        .ok()
        .map(|(info, _)| info)
}

fn check_status(response: &Response, kind: Kind, unit: u8) -> Result<(), UdpError> {
    if response.status == Status::Ok.as_u8() {
        Ok(())
    } else {
        Err(UdpError::Rejected {
            kind,
            unit,
            status: response.status,
        })
    }
}

fn expired(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}

pub(crate) fn unit_id(index: usize) -> u8 {
    u8::try_from(index).expect("at most 255 units")
}

const fn role(unit: u8) -> Role {
    if unit == GRANDMASTER {
        Role::Grandmaster
    } else {
        Role::Slave
    }
}

struct Enumerator<'a> {
    channel: &'a Channel,
    option: &'a TransportOption,
}

impl Enumerator<'_> {
    fn group_request(&mut self, kind: Kind) -> Result<u16, UdpError> {
        let group = self.channel.group();
        Ok(self.channel.request(group, kind, &[])?)
    }

    fn read_unit_info(&mut self, enough: Option<usize>) -> Result<Vec<Response>, UdpError> {
        let msg_id = self.group_request(Kind::ReadUnitInfo)?;
        self.channel.collect(
            Kind::ReadUnitInfo,
            msg_id,
            self.option.response_timeout,
            enough,
        )
    }

    fn unicast(
        &mut self,
        dst: SocketAddrV6,
        kind: Kind,
        body: &[u8],
    ) -> Result<Option<Response>, UdpError> {
        for _ in 0..UNICAST_ATTEMPTS {
            if let Some(response) =
                self.channel
                    .exchange(dst, kind, body, self.option.response_timeout)?
            {
                return Ok(Some(response));
            }
        }
        Ok(None)
    }

    fn reset(&mut self) -> Result<(), UdpError> {
        let msg_id = self.group_request(Kind::ResetId)?;
        let replies =
            self.channel
                .collect(Kind::ResetId, msg_id, self.option.response_timeout, None)?;
        tracing::debug!(replies = replies.len(), "reset the unit ids");
        std::thread::sleep(RESET_SETTLE);
        Ok(())
    }

    fn discover(&mut self) -> Result<Option<SocketAddrV6>, UdpError> {
        let msg_id = self.group_request(Kind::Discover)?;
        let Some(first) =
            self.channel
                .first(Kind::Discover, msg_id, self.option.response_timeout)?
        else {
            return Ok(None);
        };
        let others = self
            .channel
            .collect(Kind::Discover, msg_id, DISCOVER_LINGER, None)?;
        if !others.is_empty() {
            return Err(UdpError::DownstreamNotClosed {
                replies: others.len() + 1,
            });
        }
        Ok(Some(first.src))
    }

    fn assign(&mut self, target: SocketAddrV6, unit: u8) -> Result<SocketAddrV6, UdpError> {
        let body = AssignIdBody {
            unit_id: unit,
            role: role(unit).as_u8(),
        };
        if let Some(response) = self.unicast(target, Kind::AssignId, body.as_bytes())? {
            check_status(&response, Kind::AssignId, unit)?;
            return Ok(response.src);
        }
        self.read_unit_info(None)?
            .into_iter()
            .find(|r| {
                unit_info(r).is_some_and(|info| {
                    info.unit_id == unit && info.flags.contains(Flags::ASSIGNED)
                })
            })
            .map(|r| r.src)
            .ok_or(UdpError::NoResponse {
                kind: Kind::AssignId,
                unit,
            })
    }

    fn unblock(&mut self, addr: SocketAddrV6, unit: u8) -> Result<bool, UdpError> {
        let response =
            self.unicast(addr, Kind::UnblockDownstream, &[])?
                .ok_or(UdpError::NoResponse {
                    kind: Kind::UnblockDownstream,
                    unit,
                })?;
        check_status(&response, Kind::UnblockDownstream, unit)?;
        Ok(UnblockReply::read_from_prefix(&response.body)
            .is_ok_and(|(reply, _)| reply.downstream_link != 0))
    }

    fn enumerate(&mut self, expected: usize) -> Result<Vec<SocketAddrV6>, UdpError> {
        let deadline = Instant::now().checked_add(self.option.enumeration_timeout);
        let mut units: Vec<SocketAddrV6> = Vec::with_capacity(expected);
        loop {
            let Some(target) = self.discover()? else {
                if !expired(deadline) && self.release_stale(&units)? {
                    continue;
                }
                if units.len() >= expected {
                    break;
                }
                if expired(deadline) {
                    return Err(UdpError::EnumerationTimeout {
                        expected,
                        found: units.len(),
                    });
                }
                std::thread::sleep(DISCOVER_RETRY_INTERVAL);
                continue;
            };
            if units.len() >= expected {
                return Err(UdpError::DeviceCountMismatch {
                    expected,
                    found: units.len() + 1,
                });
            }
            let unit = unit_id(units.len());
            let addr = self.assign(target, unit)?;
            let downstream_link = self.unblock(addr, unit)?;
            tracing::debug!(unit, %addr, downstream_link, "assigned a unit");
            units.push(addr);
            if units.len() >= expected && !downstream_link {
                break;
            }
        }
        Ok(units)
    }

    fn release_stale(&mut self, units: &[SocketAddrV6]) -> Result<bool, UdpError> {
        let stale: Vec<SocketAddrV6> = self
            .read_unit_info(None)?
            .into_iter()
            .filter(|r| {
                unit_info(r).is_some_and(|info| info.flags.contains(Flags::ASSIGNED))
                    && !units.iter().any(|&addr| same_endpoint(addr, r.src))
            })
            .map(|r| r.src)
            .collect();
        if stale.is_empty() {
            return Ok(false);
        }
        for &addr in &stale {
            tracing::debug!(%addr, "releasing a unit left assigned by a previous enumeration");
            self.channel
                .exchange(addr, Kind::ResetId, &[], self.option.response_timeout)?;
        }
        std::thread::sleep(RESET_SETTLE);
        Ok(true)
    }

    fn cross_check(&mut self, units: &[SocketAddrV6]) -> Result<(), UdpError> {
        let mut seen = vec![false; units.len()];
        for response in self.read_unit_info(None)? {
            let info = unit_info(&response);
            let unit_id = info.map_or(u8::MAX, |info| info.unit_id);
            let known = info.is_some_and(|info| info.flags.contains(Flags::ASSIGNED))
                && units
                    .get(usize::from(unit_id))
                    .is_some_and(|&addr| same_endpoint(addr, response.src));
            if !known || seen[usize::from(unit_id)] {
                return Err(UdpError::UnexpectedUnit {
                    unit_id,
                    addr: response.src,
                });
            }
            seen[usize::from(unit_id)] = true;
        }
        match seen.iter().position(|seen| !seen) {
            Some(missing) => Err(UdpError::MissingUnit(unit_id(missing))),
            None => Ok(()),
        }
    }

    fn set_time(
        &mut self,
        grandmaster: SocketAddrV6,
        device_clock: &DeviceClock,
    ) -> Result<(), UdpError> {
        let body = SetTimeBody {
            sys_time: U64::new(DEVICE_TIME_AT_OPEN.sys_time()),
        };
        let mut answered = None;
        for _ in 0..UNICAST_ATTEMPTS {
            answered = self.channel.exchange(
                grandmaster,
                Kind::SetTime,
                body.as_bytes(),
                self.option.response_timeout,
            )?;
            if answered.is_some() {
                break;
            }
        }
        let response = answered.ok_or(UdpError::NoResponse {
            kind: Kind::SetTime,
            unit: GRANDMASTER,
        })?;
        check_status(&response, Kind::SetTime, GRANDMASTER)?;
        device_clock.observe(DEVICE_TIME_AT_OPEN);
        Ok(())
    }

    fn wait_sync(&mut self, units: &[SocketAddrV6]) -> Result<(), UdpError> {
        let deadline = Instant::now().checked_add(self.option.sync_timeout);
        loop {
            let mut ready = vec![false; units.len()];
            for response in self.read_unit_info(Some(units.len()))? {
                let Some(info) = unit_info(&response) else {
                    continue;
                };
                let index = usize::from(info.unit_id);
                if units
                    .get(index)
                    .is_some_and(|&addr| same_endpoint(addr, response.src))
                {
                    ready[index] = info.flags.contains(SYNCED);
                }
            }
            let not_ready: Vec<u8> = ready
                .iter()
                .enumerate()
                .filter(|&(_, ready)| !ready)
                .map(|(i, _)| unit_id(i))
                .collect();
            if not_ready.is_empty() {
                return Ok(());
            }
            if expired(deadline) {
                return Err(UdpError::SyncTimeout {
                    not_ready,
                    timeout: self.option.sync_timeout,
                });
            }
            std::thread::sleep(SYNC_POLL_INTERVAL);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Bringup {
    Synchronized,
    Unsynchronized,
}

pub(crate) fn bring_up(
    channel: &Channel,
    option: &TransportOption,
    expected: usize,
    bringup: Bringup,
    device_clock: &DeviceClock,
) -> Result<Vec<SocketAddrV6>, UdpError> {
    let mut enumerator = Enumerator { channel, option };
    enumerator.reset()?;
    let units = enumerator.enumerate(expected)?;
    enumerator.cross_check(&units)?;
    if bringup == Bringup::Unsynchronized {
        tracing::info!(
            units = units.len(),
            "enumerated the units without synchronizing them"
        );
        return Ok(units);
    }
    enumerator.set_time(units[usize::from(GRANDMASTER)], device_clock)?;
    enumerator.wait_sync(&units)?;
    tracing::info!(
        units = units.len(),
        "all units are locked to the grandmaster and run their sync pulse"
    );
    Ok(units)
}
