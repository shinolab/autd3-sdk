use std::net::SocketAddrV6;
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::{Flags, Kind, Role};
use autd3_rs_core::DeviceClock;
use autd3_rs_core::value::SysTime;
use zerocopy::IntoBytes;

use crate::udp::state::SYNCED_FLAGS;
use crate::udp::{
    Channel, ControlResponse, TransportOption, UdpError, assign_id_body, same_endpoint,
    set_time_body,
};

pub(super) const REQUEST_REPLY_WAIT: Duration = Duration::from_millis(200);
const EXCHANGE_ATTEMPTS: usize = 3;
const VERIFY_ASSIGNMENTS_ATTEMPTS: usize = 3;
const RESET_SETTLE: Duration = Duration::from_millis(100);
const DISCOVER_LINGER: Duration = Duration::from_millis(10);
const DISCOVER_RETRY_INTERVAL: Duration = Duration::from_millis(100);
const SYNC_POLL_INTERVAL: Duration = Duration::from_millis(50);
const GRANDMASTER_UNIT_ID: u8 = 0;
const DEVICE_TIME_AT_OPEN: SysTime = SysTime::ZERO;

fn deadline_passed(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}

fn unit_id_of(device: usize) -> u8 {
    u8::try_from(device).expect("at most 255 devices")
}

const fn role_of(unit_id: u8) -> Role {
    if unit_id == GRANDMASTER_UNIT_ID {
        Role::Grandmaster
    } else {
        Role::Slave
    }
}

pub(super) fn collect_responses(
    channel: &Channel,
    kind: Kind,
    msg_id: u16,
    wait: Duration,
    max_responses: Option<usize>,
) -> Result<Vec<ControlResponse>, UdpError> {
    let deadline = Instant::now().checked_add(wait);
    let mut responses = Vec::new();
    while max_responses.is_none_or(|n| responses.len() < n) {
        let Some(parsed) = channel.recv(deadline, |packet| {
            ControlResponse::parse(packet, kind, msg_id)
        })?
        else {
            break;
        };
        if let Some(response) = parsed? {
            responses.push(response);
        }
    }
    Ok(responses)
}

struct Enumerator<'a> {
    channel: &'a Channel,
    last_msg_id: u16,
    open_timeout: Duration,
    deadline: Option<Instant>,
}

impl Enumerator<'_> {
    fn send_request(
        &mut self,
        dst: SocketAddrV6,
        kind: Kind,
        body: &[u8],
    ) -> Result<u16, UdpError> {
        self.last_msg_id = self.last_msg_id.wrapping_add(1);
        self.channel
            .send(dst, kind, self.last_msg_id, [body, &[]], None)?;
        Ok(self.last_msg_id)
    }

    fn request_all_devices(&mut self, kind: Kind) -> Result<u16, UdpError> {
        self.send_request(self.channel.all_devices_dst(), kind, &[])
    }

    fn request_once(
        &mut self,
        dst: SocketAddrV6,
        kind: Kind,
        body: &[u8],
    ) -> Result<Option<ControlResponse>, UdpError> {
        let msg_id = self.send_request(dst, kind, body)?;
        Ok(collect_responses(self.channel, kind, msg_id, REQUEST_REPLY_WAIT, Some(1))?.pop())
    }

    fn read_unit_info(
        &mut self,
        max_responses: Option<usize>,
    ) -> Result<Vec<ControlResponse>, UdpError> {
        let msg_id = self.request_all_devices(Kind::ReadUnitInfo)?;
        collect_responses(
            self.channel,
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
            if let Some(response) = self.request_once(dst, kind, body)? {
                return Ok(Some(response));
            }
        }
        Ok(None)
    }

    fn reset_all_unit_ids(&mut self) -> Result<(), UdpError> {
        let msg_id = self.request_all_devices(Kind::ResetId)?;
        let replies = collect_responses(
            self.channel,
            Kind::ResetId,
            msg_id,
            REQUEST_REPLY_WAIT,
            None,
        )?;
        tracing::debug!(replies = replies.len(), "reset the unit ids");
        std::thread::sleep(RESET_SETTLE);
        Ok(())
    }

    fn discover(&mut self) -> Result<Option<SocketAddrV6>, UdpError> {
        let msg_id = self.request_all_devices(Kind::Discover)?;
        let Some(first) = collect_responses(
            self.channel,
            Kind::Discover,
            msg_id,
            REQUEST_REPLY_WAIT,
            Some(1),
        )?
        .pop() else {
            return Ok(None);
        };
        let others =
            collect_responses(self.channel, Kind::Discover, msg_id, DISCOVER_LINGER, None)?;
        if !others.is_empty() {
            return Err(UdpError::DownstreamNotClosed {
                replies: others.len() + 1,
            });
        }
        Ok(Some(first.src))
    }

    fn assign(&mut self, discovered: SocketAddrV6, unit_id: u8) -> Result<SocketAddrV6, UdpError> {
        let body = assign_id_body(unit_id, role_of(unit_id));
        if let Some(response) =
            self.exchange_with_retries(discovered, Kind::AssignId, body.as_bytes())?
        {
            response.check_status(Kind::AssignId, unit_id)?;
            return Ok(response.src);
        }
        self.read_unit_info(None)?
            .into_iter()
            .find(|r| {
                r.unit_info().is_some_and(|info| {
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
        response.check_status(Kind::UnblockDownstream, unit_id)?;
        Ok(response.opened_downstream_link())
    }

    fn enumerate(&mut self, num_devices: usize) -> Result<Vec<SocketAddrV6>, UdpError> {
        let deadline = self.deadline;
        let mut device_addrs: Vec<SocketAddrV6> = Vec::with_capacity(num_devices);
        loop {
            let Some(discovered) = self.discover()? else {
                if !deadline_passed(deadline) && self.release_stale_assignments(&device_addrs)? {
                    continue;
                }
                if device_addrs.len() >= num_devices {
                    break;
                }
                if deadline_passed(deadline) {
                    return Err(UdpError::EnumerationTimeout {
                        expected: num_devices,
                        found: device_addrs.len(),
                    });
                }
                std::thread::sleep(DISCOVER_RETRY_INTERVAL);
                continue;
            };
            if device_addrs.len() >= num_devices {
                return Err(UdpError::DeviceCountMismatch {
                    expected: num_devices,
                    found: device_addrs.len() + 1,
                });
            }
            let unit_id = unit_id_of(device_addrs.len());
            let addr = self.assign(discovered, unit_id)?;
            let has_downstream_link = self.unblock_downstream(addr, unit_id)?;
            tracing::debug!(unit_id, %addr, downstream_link = has_downstream_link, "assigned a unit");
            device_addrs.push(addr);
            if device_addrs.len() >= num_devices && !has_downstream_link {
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
                r.unit_info()
                    .is_some_and(|info| info.flags.contains(Flags::ASSIGNED))
                    && !device_addrs.iter().any(|&addr| same_endpoint(addr, r.src))
            })
            .map(|r| r.src)
            .collect();
        if stale.is_empty() {
            return Ok(false);
        }
        for &addr in &stale {
            tracing::debug!(%addr, "releasing a unit left assigned by a previous enumeration");
            self.request_once(addr, Kind::ResetId, &[])?;
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
            let info = response.unit_info();
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
        let body = set_time_body(DEVICE_TIME_AT_OPEN);
        let response = self
            .exchange_with_retries(grandmaster, Kind::SetTime, body.as_bytes())?
            .ok_or(UdpError::NoResponse {
                kind: Kind::SetTime,
                unit_id: GRANDMASTER_UNIT_ID,
            })?;
        response.check_status(Kind::SetTime, GRANDMASTER_UNIT_ID)?;
        device_clock.observe(DEVICE_TIME_AT_OPEN);
        Ok(())
    }

    fn wait_sync(&mut self, device_addrs: &[SocketAddrV6]) -> Result<(), UdpError> {
        let deadline = self.deadline;
        loop {
            let mut ready = vec![false; device_addrs.len()];
            for response in self.read_unit_info(Some(device_addrs.len()))? {
                let Some(info) = response.unit_info() else {
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

pub(super) fn bring_up(
    channel: &Channel,
    option: &TransportOption,
    deadline: Option<Instant>,
    num_devices: usize,
    device_clock: &DeviceClock,
) -> Result<Vec<SocketAddrV6>, UdpError> {
    let mut enumerator = Enumerator {
        channel,
        last_msg_id: 0,
        open_timeout: option.open_timeout,
        deadline,
    };
    enumerator.reset_all_unit_ids()?;
    let device_addrs = enumerator.enumerate(num_devices)?;
    enumerator.verify_assignments(&device_addrs)?;
    if !option.sync_on_open {
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
