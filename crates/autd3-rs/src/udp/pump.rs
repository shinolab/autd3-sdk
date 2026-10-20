use std::sync::Arc;
use std::time::Instant;

use super::bus::UdpBus;
use super::error::UdpError;
use super::reply::Reply;

struct HeartbeatWait {
    msg_id: u16,
    deadline: Option<Instant>,
    expected: u128,
    replied: u128,
}

pub struct Pump {
    bus: Arc<UdpBus>,
    heartbeat: Option<HeartbeatWait>,
}

impl Pump {
    #[must_use]
    pub fn new(bus: Arc<UdpBus>) -> Self {
        Self {
            bus,
            heartbeat: None,
        }
    }

    #[must_use]
    pub fn bus(&self) -> &Arc<UdpBus> {
        &self.bus
    }

    pub fn recv(&mut self) -> Result<Option<Reply>, UdpError> {
        loop {
            let Some(reply) = self.bus.recv(Instant::now())? else {
                return Ok(None);
            };
            if !self.consume(&reply) {
                return Ok(Some(reply));
            }
        }
    }

    fn consume(&mut self, reply: &Reply) -> bool {
        let Some(wait) = &mut self.heartbeat else {
            return false;
        };
        if wait.msg_id != reply.msg_id {
            return false;
        }
        wait.replied |= 1 << reply.device;
        if wait.replied & wait.expected == wait.expected {
            self.heartbeat = None;
        }
        true
    }

    pub fn maintain(&mut self, now: Instant, active: bool) -> Result<Option<Instant>, UdpError> {
        if let Some(wait) = &self.heartbeat
            && wait.deadline.is_some_and(|deadline| now >= deadline)
        {
            let missed = (wait.expected & !wait.replied).count_ones();
            if missed > 0 {
                tracing::trace!(missed, "heartbeat replies missing");
                self.bus
                    .stats()
                    .record_missed_heartbeat_replies(u64::from(missed));
            }
            self.heartbeat = None;
        }
        let interval = self.bus.heartbeat_interval().filter(|_| active);
        if let Some(interval) = interval
            && self.heartbeat.is_none()
            && now.saturating_duration_since(self.bus.last_send()) >= interval
        {
            let sent = self.bus.heartbeat()?;
            self.heartbeat = Some(HeartbeatWait {
                msg_id: sent.msg_id,
                deadline: now.checked_add(self.bus.reply_timeout().min(interval)),
                expected: sent.devices,
                replied: 0,
            });
        }
        Ok([
            interval.and_then(|interval| self.bus.last_send().checked_add(interval)),
            self.heartbeat.as_ref().and_then(|wait| wait.deadline),
            self.bus.recovery_deadline(),
        ]
        .into_iter()
        .flatten()
        .min())
    }

    #[must_use]
    pub fn settle_recovery(&self, now: Instant) -> Option<u128> {
        self.bus.settle_recovery(now)
    }
}
