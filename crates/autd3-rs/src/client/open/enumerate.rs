use std::net::SocketAddrV6;
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::{AssignIdBody, Flags, Header, Kind, Role, SetTimeBody};
use autd3_rs_core::value::SysTime;
use zerocopy::little_endian::U64;
use zerocopy::{FromBytes, IntoBytes};

use crate::udp::{
    Channel, ControlResponse, RECV_BUFFER_BYTES, TransportOption, UdpError, same_endpoint,
};

use crate::client::GRANDMASTER_UNIT_ID;
use crate::client::device_clock::DeviceClock;

pub(super) const REQUEST_REPLY_WAIT: Duration = Duration::from_millis(200);
const RESET_SETTLE: Duration = Duration::from_millis(100);
const DEVICE_TIME_AT_OPEN: SysTime = SysTime::ZERO;

fn deadline_passed(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}

pub(super) async fn collect_responses(
    channel: &Channel,
    kind: Kind,
    msg_id: u16,
    wait: Duration,
    max_responses: Option<usize>,
) -> Result<Vec<ControlResponse>, UdpError> {
    let deadline = Instant::now().checked_add(wait);
    let mut responses = Vec::new();
    let mut buf = [0; RECV_BUFFER_BYTES];
    while max_responses.is_none_or(|n| responses.len() < n) {
        let Some((src, datagram)) = channel.recv_datagram(&mut buf, deadline).await? else {
            break;
        };
        let Ok((header, body)) = Header::read_from_prefix(datagram) else {
            continue;
        };
        if let Some(response) = ControlResponse::parse(src, header, body, kind, msg_id)? {
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
    async fn send_request(
        &mut self,
        dst: SocketAddrV6,
        kind: Kind,
        body: &[u8],
    ) -> Result<u16, UdpError> {
        self.last_msg_id = self.last_msg_id.wrapping_add(1);
        self.channel
            .send(dst, kind, self.last_msg_id, [body, &[]], None)
            .await?;
        Ok(self.last_msg_id)
    }

    async fn request_all_devices(
        &mut self,
        kind: Kind,
        max_responses: Option<usize>,
    ) -> Result<Vec<ControlResponse>, UdpError> {
        let msg_id = self
            .send_request(self.channel.all_devices_dst(), kind, &[])
            .await?;
        collect_responses(
            self.channel,
            kind,
            msg_id,
            REQUEST_REPLY_WAIT,
            max_responses,
        )
        .await
    }

    async fn request_once(
        &mut self,
        dst: SocketAddrV6,
        kind: Kind,
        body: &[u8],
    ) -> Result<Option<ControlResponse>, UdpError> {
        let msg_id = self.send_request(dst, kind, body).await?;
        Ok(
            collect_responses(self.channel, kind, msg_id, REQUEST_REPLY_WAIT, Some(1))
                .await?
                .pop(),
        )
    }

    async fn exchange_with_retries(
        &mut self,
        dst: SocketAddrV6,
        kind: Kind,
        body: &[u8],
    ) -> Result<Option<ControlResponse>, UdpError> {
        for _ in 0..3 {
            if let Some(response) = self.request_once(dst, kind, body).await? {
                return Ok(Some(response));
            }
        }
        Ok(None)
    }

    async fn reset_all_unit_ids(&mut self) -> Result<(), UdpError> {
        let replies = self.request_all_devices(Kind::ResetId, None).await?;
        tracing::debug!(replies = replies.len(), "reset the unit ids");
        tokio::time::sleep(RESET_SETTLE).await;
        Ok(())
    }

    async fn discover(&mut self) -> Result<Option<SocketAddrV6>, UdpError> {
        let msg_id = self
            .send_request(self.channel.all_devices_dst(), Kind::Discover, &[])
            .await?;
        let Some(first) = collect_responses(
            self.channel,
            Kind::Discover,
            msg_id,
            REQUEST_REPLY_WAIT,
            Some(1),
        )
        .await?
        .pop() else {
            return Ok(None);
        };
        let others = collect_responses(
            self.channel,
            Kind::Discover,
            msg_id,
            Duration::from_millis(10),
            None,
        )
        .await?;
        if !others.is_empty() {
            return Err(UdpError::DownstreamNotClosed {
                replies: others.len() + 1,
            });
        }
        Ok(Some(first.src))
    }

    #[allow(clippy::cast_possible_truncation)]
    async fn assign(
        &mut self,
        discovered: SocketAddrV6,
        unit_id: usize,
    ) -> Result<SocketAddrV6, UdpError> {
        let role = if unit_id == GRANDMASTER_UNIT_ID {
            Role::Grandmaster
        } else {
            Role::Slave
        };
        let body = AssignIdBody {
            unit_id: unit_id as u8,
            role: role.as_u8(),
        };
        if let Some(response) = self
            .exchange_with_retries(discovered, Kind::AssignId, body.as_bytes())
            .await?
        {
            response.check_status(Kind::AssignId, unit_id)?;
            return Ok(response.src);
        }
        self.request_all_devices(Kind::ReadUnitInfo, None)
            .await?
            .into_iter()
            .find(|r| {
                r.unit_info().is_some_and(|info| {
                    usize::from(info.unit_id) == unit_id && info.flags.contains(Flags::ASSIGNED)
                })
            })
            .map(|r| r.src)
            .ok_or(UdpError::NoResponse {
                kind: Kind::AssignId,
                unit_id,
            })
    }

    async fn unblock_downstream(
        &mut self,
        addr: SocketAddrV6,
        unit_id: usize,
    ) -> Result<bool, UdpError> {
        let response = self
            .exchange_with_retries(addr, Kind::UnblockDownstream, &[])
            .await?
            .ok_or(UdpError::NoResponse {
                kind: Kind::UnblockDownstream,
                unit_id,
            })?;
        response.check_status(Kind::UnblockDownstream, unit_id)?;
        Ok(response.opened_downstream_link())
    }

    async fn enumerate(&mut self, num_devices: usize) -> Result<Vec<SocketAddrV6>, UdpError> {
        let deadline = self.deadline;
        let mut device_addrs: Vec<SocketAddrV6> = Vec::with_capacity(num_devices);
        loop {
            let Some(discovered) = self.discover().await? else {
                if !deadline_passed(deadline)
                    && self.release_stale_assignments(&device_addrs).await?
                {
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
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            };
            if device_addrs.len() >= num_devices {
                return Err(UdpError::DeviceCountMismatch {
                    expected: num_devices,
                    found: device_addrs.len() + 1,
                });
            }
            let unit_id = device_addrs.len();
            let addr = self.assign(discovered, unit_id).await?;
            let has_downstream_link = self.unblock_downstream(addr, unit_id).await?;
            tracing::debug!(unit_id, %addr, downstream_link = has_downstream_link, "assigned a unit");
            device_addrs.push(addr);
            if device_addrs.len() >= num_devices && !has_downstream_link {
                break;
            }
        }
        Ok(device_addrs)
    }

    async fn release_stale_assignments(
        &mut self,
        device_addrs: &[SocketAddrV6],
    ) -> Result<bool, UdpError> {
        let stale: Vec<SocketAddrV6> = self
            .request_all_devices(Kind::ReadUnitInfo, None)
            .await?
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
            self.request_once(addr, Kind::ResetId, &[]).await?;
        }
        tokio::time::sleep(RESET_SETTLE).await;
        Ok(true)
    }

    async fn verify_assignments(&mut self, device_addrs: &[SocketAddrV6]) -> Result<(), UdpError> {
        let mut answered = vec![false; device_addrs.len()];
        for _ in 0..3 {
            self.verify_assignments_once(device_addrs, &mut answered)
                .await?;
            if answered.iter().all(|&answered| answered) {
                return Ok(());
            }
        }
        match answered.iter().position(|answered| !answered) {
            Some(missing) => Err(UdpError::MissingUnit(missing)),
            None => Ok(()),
        }
    }

    async fn verify_assignments_once(
        &mut self,
        device_addrs: &[SocketAddrV6],
        answered: &mut [bool],
    ) -> Result<(), UdpError> {
        let mut seen = vec![false; device_addrs.len()];
        for response in self.request_all_devices(Kind::ReadUnitInfo, None).await? {
            let info = response.unit_info();
            let unit_id = info.map_or(usize::from(u8::MAX), |info| usize::from(info.unit_id));
            let known = info.is_some_and(|info| info.flags.contains(Flags::ASSIGNED))
                && device_addrs
                    .get(unit_id)
                    .is_some_and(|&addr| same_endpoint(addr, response.src));
            if !known || seen[unit_id] {
                return Err(UdpError::UnexpectedUnit {
                    unit_id,
                    addr: response.src,
                });
            }
            seen[unit_id] = true;
            answered[unit_id] = true;
        }
        Ok(())
    }

    async fn set_time(
        &mut self,
        grandmaster: SocketAddrV6,
        device_clock: &mut DeviceClock,
    ) -> Result<(), UdpError> {
        let body = SetTimeBody {
            sys_time: U64::new(DEVICE_TIME_AT_OPEN.sys_time()),
        };
        let response = self
            .exchange_with_retries(grandmaster, Kind::SetTime, body.as_bytes())
            .await?
            .ok_or(UdpError::NoResponse {
                kind: Kind::SetTime,
                unit_id: GRANDMASTER_UNIT_ID,
            })?;
        response.check_status(Kind::SetTime, GRANDMASTER_UNIT_ID)?;
        device_clock.observe(DEVICE_TIME_AT_OPEN, Instant::now());
        Ok(())
    }

    async fn wait_sync(&mut self, device_addrs: &[SocketAddrV6]) -> Result<(), UdpError> {
        let deadline = self.deadline;
        loop {
            let mut ready = vec![false; device_addrs.len()];
            for response in self
                .request_all_devices(Kind::ReadUnitInfo, Some(device_addrs.len()))
                .await?
            {
                let Some(info) = response.unit_info() else {
                    continue;
                };
                let device = usize::from(info.unit_id);
                if device_addrs
                    .get(device)
                    .is_some_and(|&addr| same_endpoint(addr, response.src))
                {
                    ready[device] = info.flags.contains(Flags::PTP_LOCKED | Flags::SYNC_READY);
                }
            }
            let not_ready: Vec<usize> = ready
                .iter()
                .enumerate()
                .filter(|&(_, ready)| !ready)
                .map(|(device, _)| device)
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
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

pub(super) async fn bring_up(
    channel: &Channel,
    option: &TransportOption,
    deadline: Option<Instant>,
    num_devices: usize,
    device_clock: &mut DeviceClock,
) -> Result<Vec<SocketAddrV6>, UdpError> {
    let mut enumerator = Enumerator {
        channel,
        last_msg_id: 0,
        open_timeout: option.open_timeout,
        deadline,
    };
    enumerator.reset_all_unit_ids().await?;
    let device_addrs = enumerator.enumerate(num_devices).await?;
    enumerator.verify_assignments(&device_addrs).await?;
    if !option.sync_on_open {
        tracing::info!(
            units = device_addrs.len(),
            "enumerated the units without synchronizing them"
        );
        return Ok(device_addrs);
    }
    enumerator
        .set_time(device_addrs[GRANDMASTER_UNIT_ID], device_clock)
        .await?;
    enumerator.wait_sync(&device_addrs).await?;
    tracing::info!(
        units = device_addrs.len(),
        "all units are locked to the grandmaster and run their sync pulse"
    );
    Ok(device_addrs)
}
