#[cfg(test)]
mod tests;

use zerocopy::little_endian::U64;
use zerocopy::{FromBytes, IntoBytes};

use autd3_cpu_wire::udp::{
    ALL_NODES, AssignIdBody, FLAG_ASSIGNED, FLAG_DOWNSTREAM_LINK, FLAG_DOWNSTREAM_OPEN,
    FLAG_SYNC_READY, FrameReply, HEADER_BYTES, Header, Kind, PORT, PROTOCOL_VERSION,
    RESET_ID_CLOSE_DELAY_MS, SetTimeBody, Status, UNASSIGNED_ID, UPSTREAM_UNKNOWN, UnblockReply,
    UnitInfo, link_local, mac, solicited_node,
};

use crate::net::{self, Endpoint, Ipv6, Mac, NDP_HOP_LIMIT, Packet, UDP_PAYLOAD_OFFSET, Udp};
use crate::nic::{Nic, RxMeta, other_port};
use crate::proto::{HOST_TO_DEVICE_BYTES, TxFrame};
use crate::version::{FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH};

pub const TX_BUF_BYTES: usize = 320;
const MAX_ECHO_BODY: usize = TX_BUF_BYTES - UDP_PAYLOAD_OFFSET - 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Received {
    Nothing,
    HostFrame,
}

pub struct Node {
    unit_id: u8,
    upstream: Option<u8>,
    open: bool,
    downstream_link: bool,
    close_at: Option<u32>,
    mac: Mac,
    addr: Ipv6,
    buf: [u8; TX_BUF_BYTES],
}

impl Default for Node {
    fn default() -> Self {
        Self::new()
    }
}

struct Reply {
    port: u8,
    to: Endpoint,
    kind: u8,
    msg_id: u16,
}

impl Node {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            unit_id: UNASSIGNED_ID,
            upstream: None,
            open: false,
            downstream_link: false,
            close_at: None,
            mac: mac(UNASSIGNED_ID),
            addr: link_local(mac(UNASSIGNED_ID)),
            buf: [0; TX_BUF_BYTES],
        }
    }

    pub fn init<N: Nic>(&mut self, nic: &mut N) {
        self.unassign(nic);
    }

    #[must_use]
    pub const fn unit_id(&self) -> u8 {
        self.unit_id
    }

    #[must_use]
    pub const fn is_assigned(&self) -> bool {
        self.unit_id != UNASSIGNED_ID
    }

    #[must_use]
    pub const fn mac(&self) -> Mac {
        self.mac
    }

    #[must_use]
    pub const fn address(&self) -> Ipv6 {
        self.addr
    }

    pub fn flags<N: Nic>(&self, nic: &mut N) -> u8 {
        let mut flags = 0;
        if self.is_assigned() {
            flags |= FLAG_ASSIGNED;
        }
        if self.open {
            flags |= FLAG_DOWNSTREAM_OPEN;
        }
        if self.downstream_link {
            flags |= FLAG_DOWNSTREAM_LINK;
        }
        if nic.pulse_ready() {
            flags |= FLAG_SYNC_READY;
        }
        flags
    }

    fn unassign<N: Nic>(&mut self, nic: &mut N) {
        nic.set_forwarding(false);
        self.unit_id = UNASSIGNED_ID;
        self.upstream = None;
        self.open = false;
        self.downstream_link = false;
        self.close_at = None;
        self.mac = mac(UNASSIGNED_ID);
        self.addr = link_local(self.mac);
        nic.set_mac(self.mac);
        nic.stop_pulse();
    }

    pub fn tick<N: Nic>(&mut self, nic: &mut N, now_ms: u32) {
        if let Some(at) = self.close_at
            && now_ms.wrapping_sub(at).cast_signed() >= 0
        {
            self.unassign(nic);
        }
    }

    pub fn on_frame<N: Nic, F: FnOnce(&[u8; HOST_TO_DEVICE_BYTES]) -> TxFrame>(
        &mut self,
        nic: &mut N,
        now_ms: u32,
        frame: &[u8],
        rx: RxMeta,
        data: F,
    ) -> Received {
        match net::parse(frame) {
            Some(Packet::NeighborSolicitation {
                src_mac,
                src_ip,
                dst_ip,
                target,
                hop_limit,
            }) => {
                if hop_limit == NDP_HOP_LIMIT
                    && target == self.addr
                    && src_ip != net::UNSPECIFIED
                    && (dst_ip == self.addr || dst_ip == solicited_node(self.addr))
                    && let Some(len) = net::neighbor_advertisement(
                        &mut self.buf,
                        self.mac,
                        self.addr,
                        src_mac,
                        src_ip,
                    )
                {
                    nic.send(&self.buf[..len], rx.port);
                }
                Received::Nothing
            }
            Some(Packet::EchoRequest {
                src_mac,
                src_ip,
                dst_ip,
                body,
            }) => {
                if dst_ip == self.addr
                    && body.len() <= MAX_ECHO_BODY
                    && let Some(len) =
                        net::echo_reply(&mut self.buf, self.mac, self.addr, src_mac, src_ip, body)
                {
                    nic.send(&self.buf[..len], rx.port);
                }
                Received::Nothing
            }
            Some(Packet::Udp(udp)) => {
                if udp.dst_port == PORT && (udp.dst_ip == self.addr || udp.dst_ip == ALL_NODES) {
                    self.on_udp(nic, now_ms, &udp, rx, data)
                } else {
                    Received::Nothing
                }
            }
            None => Received::Nothing,
        }
    }

    fn on_udp<N: Nic, F: FnOnce(&[u8; HOST_TO_DEVICE_BYTES]) -> TxFrame>(
        &mut self,
        nic: &mut N,
        now_ms: u32,
        udp: &Udp<'_>,
        rx: RxMeta,
        data: F,
    ) -> Received {
        let Ok((header, body)) = Header::ref_from_prefix(udp.payload) else {
            return Received::Nothing;
        };
        let reply = Reply {
            port: rx.port,
            to: Endpoint {
                mac: udp.src_mac,
                ip: udp.src_ip,
                port: udp.src_port,
            },
            kind: header.kind,
            msg_id: header.msg_id.get(),
        };
        if header.version != PROTOCOL_VERSION {
            self.status(nic, &reply, Status::UnsupportedVersion);
            return Received::Nothing;
        }
        let unicast = udp.dst_ip == self.addr;
        match Kind::from_u8(header.kind) {
            Some(Kind::Frame) => {
                let Some(frame) = body
                    .get(..HOST_TO_DEVICE_BYTES)
                    .and_then(|b| <&[u8; HOST_TO_DEVICE_BYTES]>::try_from(b).ok())
                else {
                    return Received::Nothing;
                };
                if !self.is_assigned() || !unicast {
                    return Received::Nothing;
                }
                let tx = data(frame);
                let body = FrameReply {
                    ack: tx.ack,
                    data: tx.data,
                    flags: self.flags(nic),
                    unit_id: self.unit_id,
                    sys_time: U64::new(nic.now().unwrap_or(0)),
                };
                self.send_reply(nic, &reply, None, body.as_bytes());
                return Received::HostFrame;
            }
            Some(Kind::ReadUnitInfo) => {
                let info = self.unit_info(nic);
                self.send_reply(nic, &reply, Some(Status::Ok), info.as_bytes());
            }
            Some(Kind::ResetId) => {
                self.status(nic, &reply, Status::Ok);
                if self.close_at.is_none() {
                    self.close_at = Some(now_ms.wrapping_add(RESET_ID_CLOSE_DELAY_MS));
                }
            }
            Some(Kind::Discover) => {
                if !self.is_assigned() {
                    self.status(nic, &reply, Status::Ok);
                }
            }
            Some(Kind::AssignId) if unicast && !self.is_assigned() => {
                self.assign(nic, body, &reply, rx.port);
            }
            Some(Kind::UnblockDownstream) if unicast => self.unblock(nic, &reply),
            Some(Kind::SetTime) if unicast => {
                let status = self.set_time(nic, body);
                self.status(nic, &reply, status);
            }
            _ => {}
        }
        Received::Nothing
    }

    fn assign<N: Nic>(&mut self, nic: &mut N, body: &[u8], reply: &Reply, port: u8) {
        let Ok((req, _)) = AssignIdBody::ref_from_prefix(body) else {
            self.status(nic, reply, Status::InvalidPayload);
            return;
        };
        if req.unit_id == UNASSIGNED_ID {
            self.status(nic, reply, Status::InvalidPayload);
            return;
        }
        self.unit_id = req.unit_id;
        self.upstream = Some(port & 1);
        self.close_at = None;
        self.mac = mac(req.unit_id);
        self.addr = link_local(self.mac);
        nic.set_mac(self.mac);
        self.status(nic, reply, Status::Ok);
    }

    fn unblock<N: Nic>(&mut self, nic: &mut N, reply: &Reply) {
        let Some(upstream) = self.upstream.filter(|_| self.is_assigned()) else {
            self.status(nic, reply, Status::NotAssigned);
            return;
        };
        nic.set_forwarding(true);
        self.open = true;
        self.downstream_link = nic.downstream_link(other_port(upstream));
        let body = UnblockReply {
            downstream_link: u8::from(self.downstream_link),
        };
        self.send_reply(nic, reply, Some(Status::Ok), body.as_bytes());
    }

    fn set_time<N: Nic>(&mut self, nic: &mut N, body: &[u8]) -> Status {
        if !self.is_assigned() {
            return Status::NotAssigned;
        }
        let Ok((req, _)) = SetTimeBody::ref_from_prefix(body) else {
            return Status::InvalidPayload;
        };
        nic.stop_pulse();
        if !nic.set_time(req.sys_time.get()) {
            return Status::InvalidPayload;
        }
        nic.arm_pulse();
        Status::Ok
    }

    fn unit_info<N: Nic>(&self, nic: &mut N) -> UnitInfo {
        UnitInfo {
            unit_id: self.unit_id,
            flags: self.flags(nic),
            upstream_port: self.upstream.unwrap_or(UPSTREAM_UNKNOWN),
            reserved: 0,
            fw_version: [FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH],
            reserved2: 0,
            sys_time: U64::new(nic.now().unwrap_or(0)),
        }
    }

    fn status<N: Nic>(&mut self, nic: &mut N, reply: &Reply, status: Status) {
        self.send_reply(nic, reply, Some(status), &[]);
    }

    fn send_reply<N: Nic>(
        &mut self,
        nic: &mut N,
        reply: &Reply,
        status: Option<Status>,
        body: &[u8],
    ) {
        let header = Header {
            version: PROTOCOL_VERSION,
            kind: reply.kind,
            msg_id: zerocopy::little_endian::U16::new(reply.msg_id),
        };
        let mut at = UDP_PAYLOAD_OFFSET;
        self.buf[at..at + HEADER_BYTES].copy_from_slice(header.as_bytes());
        at += HEADER_BYTES;
        if let Some(status) = status {
            self.buf[at] = status.as_u8();
            at += 1;
        }
        self.buf[at..at + body.len()].copy_from_slice(body);
        at += body.len();
        let src = Endpoint {
            mac: self.mac,
            ip: self.addr,
            port: PORT,
        };
        if let Some(len) = net::udp(&mut self.buf, &src, &reply.to, at - UDP_PAYLOAD_OFFSET) {
            nic.send(&self.buf[..len], reply.port);
        }
    }
}
