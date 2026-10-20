use std::net::SocketAddrV6;
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::Kind;

use crate::udp::{Channel, ControlResponse, UdpError};

pub(super) const REQUEST_REPLY_WAIT: Duration = Duration::from_millis(200);

pub(crate) struct Requester<'a> {
    channel: &'a Channel,
    last_msg_id: u16,
}

impl<'a> Requester<'a> {
    pub(crate) const fn new(channel: &'a Channel) -> Self {
        Self {
            channel,
            last_msg_id: 0,
        }
    }

    fn next_msg_id(&mut self) -> u16 {
        self.last_msg_id = self.last_msg_id.wrapping_add(1);
        self.last_msg_id
    }

    fn send_request(
        &mut self,
        dst: SocketAddrV6,
        kind: Kind,
        body: &[u8],
    ) -> Result<u16, UdpError> {
        let msg_id = self.next_msg_id();
        self.channel
            .send(dst, kind, msg_id, [body, &[]], None, || {})?;
        Ok(msg_id)
    }

    pub(crate) fn request_all_devices(&mut self, kind: Kind) -> Result<u16, UdpError> {
        let all_devices_dst = self.channel.all_devices_dst();
        self.send_request(all_devices_dst, kind, &[])
    }

    pub(crate) fn collect_responses(
        &self,
        kind: Kind,
        msg_id: u16,
        wait: Duration,
        max_responses: Option<usize>,
    ) -> Result<Vec<ControlResponse>, UdpError> {
        let deadline = Instant::now().checked_add(wait);
        let mut responses = Vec::new();
        while max_responses.is_none_or(|n| responses.len() < n) {
            let Some(parsed) = self.channel.recv(deadline, |packet| {
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

    pub(crate) fn request_once(
        &mut self,
        dst: SocketAddrV6,
        kind: Kind,
        body: &[u8],
        timeout: Duration,
    ) -> Result<Option<ControlResponse>, UdpError> {
        let msg_id = self.send_request(dst, kind, body)?;
        Ok(self
            .collect_responses(kind, msg_id, timeout, Some(1))?
            .pop())
    }
}
