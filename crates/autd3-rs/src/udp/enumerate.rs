use std::net::SocketAddrV6;
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::{
    AssignIdBody, Flags, Kind, Role, SetTimeBody, Status, UnblockReply, UnitInfo,
};
use autd3_rs_core::value::SysTime;
use autd3_rs_core::{DeviceClock, MAX_DEVICES};
use zerocopy::little_endian::U64;
use zerocopy::{FromBytes, IntoBytes};

use super::channel::{Channel, ControlResponse};
use super::error::UdpError;
use super::option::TransportOption;
use super::state::SYNCED_FLAGS;

pub(crate) const REQUEST_REPLY_WAIT: Duration = Duration::from_millis(200);
const EXCHANGE_ATTEMPTS: usize = 3;
const VERIFY_ASSIGNMENTS_ATTEMPTS: usize = 3;
const RESET_SETTLE: Duration = Duration::from_millis(100);
const DISCOVER_LINGER: Duration = Duration::from_millis(10);
const DISCOVER_RETRY_INTERVAL: Duration = Duration::from_millis(100);
const SYNC_POLL_INTERVAL: Duration = Duration::from_millis(50);
const GRANDMASTER_UNIT_ID: u8 = 0;
const DEVICE_TIME_AT_OPEN: SysTime = SysTime::ZERO;

pub(crate) fn same_endpoint(a: SocketAddrV6, b: SocketAddrV6) -> bool {
    a.ip() == b.ip() && a.port() == b.port()
}

fn parse_unit_info(response: &ControlResponse) -> Option<UnitInfo> {
    UnitInfo::read_from_prefix(&response.body)
        .ok()
        .map(|(info, _)| info)
}

fn check_status(response: &ControlResponse, kind: Kind, unit_id: u8) -> Result<(), UdpError> {
    if response.status == Status::Ok.as_u8() {
        Ok(())
    } else {
        Err(UdpError::Rejected {
            kind,
            unit_id,
            status: response.status,
        })
    }
}

fn deadline_passed(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}

pub(crate) fn unit_id_of(device: usize) -> u8 {
    u8::try_from(device).expect("at most 255 devices")
}

const fn role_of(unit_id: u8) -> Role {
    if unit_id == GRANDMASTER_UNIT_ID {
        Role::Grandmaster
    } else {
        Role::Slave
    }
}

struct Enumerator<'a> {
    channel: &'a Channel,
    open_timeout: Duration,
    deadline: Option<Instant>,
}

impl Enumerator<'_> {
    fn request_all_devices(&mut self, kind: Kind) -> Result<u16, UdpError> {
        let all_devices_dst = self.channel.all_devices_dst();
        Ok(self.channel.send_request(all_devices_dst, kind, &[])?)
    }

    fn read_unit_info(
        &mut self,
        max_responses: Option<usize>,
    ) -> Result<Vec<ControlResponse>, UdpError> {
        let msg_id = self.request_all_devices(Kind::ReadUnitInfo)?;
        self.channel.collect_responses(
            Kind::ReadUnitInfo,
            msg_id,
            REQUEST_REPLY_WAIT,
            max_responses,
        )
    }

    fn exchange_with_retries(
        &mut self,
        dst: SocketAddrV6,
        kind: Kind,
        body: &[u8],
    ) -> Result<Option<ControlResponse>, UdpError> {
        for _ in 0..EXCHANGE_ATTEMPTS {
            if let Some(response) =
                self.channel
                    .request_once(dst, kind, body, REQUEST_REPLY_WAIT)?
            {
                return Ok(Some(response));
            }
        }
        Ok(None)
    }

    fn reset_all_unit_ids(&mut self) -> Result<(), UdpError> {
        let msg_id = self.request_all_devices(Kind::ResetId)?;
        let replies =
            self.channel
                .collect_responses(Kind::ResetId, msg_id, REQUEST_REPLY_WAIT, None)?;
        tracing::debug!(replies = replies.len(), "reset the unit ids");
        std::thread::sleep(RESET_SETTLE);
        Ok(())
    }

    fn discover(&mut self) -> Result<Option<SocketAddrV6>, UdpError> {
        let msg_id = self.request_all_devices(Kind::Discover)?;
        let Some(first) = self
            .channel
            .collect_responses(Kind::Discover, msg_id, REQUEST_REPLY_WAIT, Some(1))?
            .pop()
        else {
            return Ok(None);
        };
        let others =
            self.channel
                .collect_responses(Kind::Discover, msg_id, DISCOVER_LINGER, None)?;
        if !others.is_empty() {
            return Err(UdpError::DownstreamNotClosed {
                replies: others.len() + 1,
            });
        }
        Ok(Some(first.src))
    }

    fn assign(&mut self, discovered: SocketAddrV6, unit_id: u8) -> Result<SocketAddrV6, UdpError> {
        let body = AssignIdBody {
            unit_id,
            role: role_of(unit_id).as_u8(),
        };
        if let Some(response) =
            self.exchange_with_retries(discovered, Kind::AssignId, body.as_bytes())?
        {
            check_status(&response, Kind::AssignId, unit_id)?;
            return Ok(response.src);
        }
        self.read_unit_info(None)?
            .into_iter()
            .find(|r| {
                parse_unit_info(r).is_some_and(|info| {
                    info.unit_id == unit_id && info.flags.contains(Flags::ASSIGNED)
                })
            })
            .map(|r| r.src)
            .ok_or(UdpError::NoResponse {
                kind: Kind::AssignId,
                unit_id,
            })
    }

    fn unblock_downstream(&mut self, addr: SocketAddrV6, unit_id: u8) -> Result<bool, UdpError> {
        let response = self
            .exchange_with_retries(addr, Kind::UnblockDownstream, &[])?
            .ok_or(UdpError::NoResponse {
                kind: Kind::UnblockDownstream,
                unit_id,
            })?;
        check_status(&response, Kind::UnblockDownstream, unit_id)?;
        Ok(UnblockReply::read_from_prefix(&response.body)
            .is_ok_and(|(reply, _)| reply.downstream_link != 0))
    }

    fn enumerate(
        &mut self,
        min_count: usize,
        max_count: usize,
    ) -> Result<Vec<SocketAddrV6>, UdpError> {
        let deadline = self.deadline;
        let mut device_addrs: Vec<SocketAddrV6> = Vec::with_capacity(min_count);
        loop {
            let Some(discovered) = self.discover()? else {
                if !deadline_passed(deadline) && self.release_stale_assignments(&device_addrs)? {
                    continue;
                }
                if device_addrs.len() >= min_count {
                    break;
                }
                if deadline_passed(deadline) {
                    return Err(UdpError::EnumerationTimeout {
                        expected: min_count,
                        found: device_addrs.len(),
                    });
                }
                std::thread::sleep(DISCOVER_RETRY_INTERVAL);
                continue;
            };
            if device_addrs.len() >= max_count {
                return Err(UdpError::DeviceCountMismatch {
                    expected: max_count,
                    found: device_addrs.len() + 1,
                });
            }
            let unit_id = unit_id_of(device_addrs.len());
            let addr = self.assign(discovered, unit_id)?;
            let has_downstream_link = self.unblock_downstream(addr, unit_id)?;
            tracing::debug!(unit_id, %addr, downstream_link = has_downstream_link, "assigned a unit");
            device_addrs.push(addr);
            if device_addrs.len() >= min_count && !has_downstream_link {
                break;
            }
        }
        Ok(device_addrs)
    }

    fn release_stale_assignments(
        &mut self,
        device_addrs: &[SocketAddrV6],
    ) -> Result<bool, UdpError> {
        let stale: Vec<SocketAddrV6> = self
            .read_unit_info(None)?
            .into_iter()
            .filter(|r| {
                parse_unit_info(r).is_some_and(|info| info.flags.contains(Flags::ASSIGNED))
                    && !device_addrs.iter().any(|&addr| same_endpoint(addr, r.src))
            })
            .map(|r| r.src)
            .collect();
        if stale.is_empty() {
            return Ok(false);
        }
        for &addr in &stale {
            tracing::debug!(%addr, "releasing a unit left assigned by a previous enumeration");
            self.channel
                .request_once(addr, Kind::ResetId, &[], REQUEST_REPLY_WAIT)?;
        }
        std::thread::sleep(RESET_SETTLE);
        Ok(true)
    }

    fn verify_assignments(&mut self, device_addrs: &[SocketAddrV6]) -> Result<(), UdpError> {
        let mut answered = vec![false; device_addrs.len()];
        for _ in 0..VERIFY_ASSIGNMENTS_ATTEMPTS {
            self.verify_assignments_once(device_addrs, &mut answered)?;
            if answered.iter().all(|&answered| answered) {
                return Ok(());
            }
        }
        match answered.iter().position(|answered| !answered) {
            Some(missing) => Err(UdpError::MissingUnit(unit_id_of(missing))),
            None => Ok(()),
        }
    }

    fn verify_assignments_once(
        &mut self,
        device_addrs: &[SocketAddrV6],
        answered: &mut [bool],
    ) -> Result<(), UdpError> {
        let mut seen = vec![false; device_addrs.len()];
        for response in self.read_unit_info(None)? {
            let info = parse_unit_info(&response);
            let unit_id = info.map_or(u8::MAX, |info| info.unit_id);
            let known = info.is_some_and(|info| info.flags.contains(Flags::ASSIGNED))
                && device_addrs
                    .get(usize::from(unit_id))
                    .is_some_and(|&addr| same_endpoint(addr, response.src));
            if !known || seen[usize::from(unit_id)] {
                return Err(UdpError::UnexpectedUnit {
                    unit_id,
                    addr: response.src,
                });
            }
            seen[usize::from(unit_id)] = true;
            answered[usize::from(unit_id)] = true;
        }
        Ok(())
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
        for _ in 0..EXCHANGE_ATTEMPTS {
            answered = self.channel.request_once(
                grandmaster,
                Kind::SetTime,
                body.as_bytes(),
                REQUEST_REPLY_WAIT,
            )?;
            if answered.is_some() {
                break;
            }
        }
        let response = answered.ok_or(UdpError::NoResponse {
            kind: Kind::SetTime,
            unit_id: GRANDMASTER_UNIT_ID,
        })?;
        check_status(&response, Kind::SetTime, GRANDMASTER_UNIT_ID)?;
        device_clock.observe(DEVICE_TIME_AT_OPEN);
        Ok(())
    }

    fn wait_sync(&mut self, device_addrs: &[SocketAddrV6]) -> Result<(), UdpError> {
        let deadline = self.deadline;
        loop {
            let mut ready = vec![false; device_addrs.len()];
            for response in self.read_unit_info(Some(device_addrs.len()))? {
                let Some(info) = parse_unit_info(&response) else {
                    continue;
                };
                let device = usize::from(info.unit_id);
                if device_addrs
                    .get(device)
                    .is_some_and(|&addr| same_endpoint(addr, response.src))
                {
                    ready[device] = info.flags.contains(SYNCED_FLAGS);
                }
            }
            let not_ready: Vec<u8> = ready
                .iter()
                .enumerate()
                .filter(|&(_, ready)| !ready)
                .map(|(device, _)| unit_id_of(device))
                .collect();
            if not_ready.is_empty() {
                return Ok(());
            }
            if deadline_passed(deadline) {
                return Err(UdpError::SyncTimeout {
                    not_ready,
                    timeout: self.open_timeout,
                });
            }
            std::thread::sleep(SYNC_POLL_INTERVAL);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExpectedCount {
    Exactly(usize),
    All,
}

impl ExpectedCount {
    const fn at_least(self) -> usize {
        match self {
            Self::Exactly(count) => count,
            Self::All => 1,
        }
    }

    const fn at_most(self) -> usize {
        match self {
            Self::Exactly(count) => count,
            Self::All => MAX_DEVICES,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncOnOpen {
    Synchronize,
    Skip,
}

pub(crate) fn bring_up(
    channel: &Channel,
    option: &TransportOption,
    deadline: Option<Instant>,
    expected: ExpectedCount,
    sync_on_open: SyncOnOpen,
    device_clock: &DeviceClock,
) -> Result<Vec<SocketAddrV6>, UdpError> {
    let mut enumerator = Enumerator {
        channel,
        open_timeout: option.open_timeout,
        deadline,
    };
    enumerator.reset_all_unit_ids()?;
    let device_addrs = enumerator.enumerate(expected.at_least(), expected.at_most())?;
    enumerator.verify_assignments(&device_addrs)?;
    if sync_on_open == SyncOnOpen::Skip {
        tracing::info!(
            units = device_addrs.len(),
            "enumerated the units without synchronizing them"
        );
        return Ok(device_addrs);
    }
    enumerator.set_time(device_addrs[usize::from(GRANDMASTER_UNIT_ID)], device_clock)?;
    enumerator.wait_sync(&device_addrs)?;
    tracing::info!(
        units = device_addrs.len(),
        "all units are locked to the grandmaster and run their sync pulse"
    );
    Ok(device_addrs)
}
