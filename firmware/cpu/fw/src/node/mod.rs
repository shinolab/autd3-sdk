#[cfg(test)]
mod tests;

use zerocopy::little_endian::{I32, U64};
use zerocopy::{FromBytes, IntoBytes};

use autd3_cpu_wire::udp::{
    ALL_NODES, AssignIdBody, FLAG_ASSIGNED, FLAG_DOWNSTREAM_LINK, FLAG_DOWNSTREAM_OPEN,
    FLAG_GRANDMASTER, FLAG_PTP_LOCKED, FLAG_SYNC_READY, FrameReply, HEADER_BYTES, Header, Kind,
    PORT, PROTOCOL_VERSION, RESET_ID_CLOSE_DELAY_MS, ROLE_GRANDMASTER, ROLE_SLAVE, SetTimeBody,
    Status, UNASSIGNED_ID, UPSTREAM_UNKNOWN, UnblockReply, UnitInfo, link_local, mac,
    solicited_node,
};

use crate::net::{self, Endpoint, Ipv6, Mac, NDP_HOP_LIMIT, Packet, UDP_PAYLOAD_OFFSET, Udp};
use crate::nic::{Nic, RxMeta, other_port};
use crate::proto::{Disposition, FRAME_BYTES_MAX, FRAME_HEADER_BYTES, REPLY_DATA_BYTES_MAX, Reply};
use crate::ptp::{Event, Ptp, Role};
use crate::version::{FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH};

pub const TX_BUF_BYTES: usize = 320;
const MAX_ECHO_BODY: usize = TX_BUF_BYTES - UDP_PAYLOAD_OFFSET - 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Received {
    Nothing,
    HostMessage,
}

pub trait CommandLayer {
    fn recv_frame(&mut self, frame: &[u8], msg_id: u16) -> Disposition;

    fn reply(&mut self) -> Reply;
}

const FRAME_REPLY_HEAD_BYTES: usize = core::mem::size_of::<FrameReply>();

const _: () = assert!(TX_BUF_BYTES >= UDP_PAYLOAD_OFFSET + autd3_cpu_wire::udp::MAX_REPLY_BYTES);

pub struct Node {
    unit_id: u8,
    grandmaster: bool,
    upstream: Option<u8>,
    open: bool,
    downstream_link: bool,
    close_at: Option<u32>,
    mac: Mac,
    addr: Ipv6,
    host: Option<(u8, Endpoint)>,
    ptp: Ptp,
    buf: [u8; TX_BUF_BYTES],
}

impl Default for Node {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
struct ReplyTo {
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
            grandmaster: false,
            upstream: None,
            open: false,
            downstream_link: false,
            close_at: None,
            mac: mac(UNASSIGNED_ID),
            addr: link_local(mac(UNASSIGNED_ID)),
            host: None,
            ptp: Ptp::new(),
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

    #[must_use]
    pub const fn is_grandmaster(&self) -> bool {
        self.grandmaster
    }

    #[must_use]
    pub const fn is_locked(&self) -> bool {
        self.ptp.is_locked()
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
        if self.ptp.is_locked() {
            flags |= FLAG_PTP_LOCKED;
        }
        if self.grandmaster {
            flags |= FLAG_GRANDMASTER;
        }
        flags
    }

    fn unassign<N: Nic>(&mut self, nic: &mut N) {
        nic.set_forwarding(false);
        self.unit_id = UNASSIGNED_ID;
        self.grandmaster = false;
        self.upstream = None;
        self.open = false;
        self.downstream_link = false;
        self.close_at = None;
        self.mac = mac(UNASSIGNED_ID);
        self.addr = link_local(self.mac);
        self.host = None;
        nic.set_mac(self.mac);
        self.ptp.reset(nic);
        nic.stop_pulse();
    }

    pub fn tick<N: Nic>(&mut self, nic: &mut N, now_ms: u32) {
        if let Some(at) = self.close_at
            && now_ms.wrapping_sub(at).cast_signed() >= 0
        {
            self.unassign(nic);
        }
        let event = self.ptp.tick(nic, now_ms);
        Self::apply(nic, event);
    }

    fn apply<N: Nic>(nic: &mut N, event: Event) {
        match event {
            Event::Locked => nic.arm_pulse(),
            Event::Lost => nic.stop_pulse(),
            Event::Stepped | Event::None => {}
        }
    }

    pub fn send_completion<N: Nic>(&mut self, nic: &mut N, msg_id: u16, reply: &Reply) {
        let Some((port, to)) = self.host.filter(|_| self.is_assigned()) else {
            return;
        };
        let reply_to = ReplyTo {
            port,
            to,
            kind: Kind::Frame.as_u8(),
            msg_id,
        };
        self.frame_reply(nic, &reply_to, reply);
    }

    pub fn on_frame<N: Nic, C: CommandLayer>(
        &mut self,
        nic: &mut N,
        now_ms: u32,
        frame: &[u8],
        rx: RxMeta,
        cmds: &mut C,
    ) -> Received {
        match net::parse(frame) {
            Some(Packet::Ptp { message, .. }) => {
                if self.is_assigned() {
                    let event = self.ptp.on_message(nic, message, rx, now_ms);
                    Self::apply(nic, event);
                }
                Received::Nothing
            }
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
                    nic.send(&self.buf[..len], rx.port, false);
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
                    nic.send(&self.buf[..len], rx.port, false);
                }
                Received::Nothing
            }
            Some(Packet::Udp(udp)) => {
                if udp.dst_port == PORT && (udp.dst_ip == self.addr || udp.dst_ip == ALL_NODES) {
                    self.on_udp(nic, now_ms, &udp, rx, cmds)
                } else {
                    Received::Nothing
                }
            }
            None => Received::Nothing,
        }
    }

    fn on_udp<N: Nic, C: CommandLayer>(
        &mut self,
        nic: &mut N,
        now_ms: u32,
        udp: &Udp<'_>,
        rx: RxMeta,
        cmds: &mut C,
    ) -> Received {
        let Ok((header, body)) = Header::ref_from_prefix(udp.payload) else {
            return Received::Nothing;
        };
        let reply = ReplyTo {
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
                if !self.is_assigned()
                    || !unicast
                    || !(FRAME_HEADER_BYTES..=FRAME_BYTES_MAX).contains(&body.len())
                {
                    return Received::Nothing;
                }
                self.host = Some((reply.port, reply.to));
                if cmds.recv_frame(body, reply.msg_id) == Disposition::Reply {
                    let state = cmds.reply();
                    self.frame_reply(nic, &reply, &state);
                }
                return Received::HostMessage;
            }
            Some(Kind::Heartbeat) => {
                if !self.is_assigned() || !unicast {
                    return Received::Nothing;
                }
                self.host = Some((reply.port, reply.to));
                let state = cmds.reply();
                self.frame_reply(nic, &reply, &state);
                return Received::HostMessage;
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

    fn assign<N: Nic>(&mut self, nic: &mut N, body: &[u8], reply: &ReplyTo, port: u8) {
        let Ok((req, _)) = AssignIdBody::ref_from_prefix(body) else {
            self.status(nic, reply, Status::InvalidPayload);
            return;
        };
        let role = match req.role {
            ROLE_GRANDMASTER => Role::Master,
            ROLE_SLAVE => Role::Slave,
            _ => {
                self.status(nic, reply, Status::InvalidPayload);
                return;
            }
        };
        if req.unit_id == UNASSIGNED_ID {
            self.status(nic, reply, Status::InvalidPayload);
            return;
        }
        self.unit_id = req.unit_id;
        self.grandmaster = role == Role::Master;
        self.upstream = Some(port & 1);
        self.close_at = None;
        self.mac = mac(req.unit_id);
        self.addr = link_local(self.mac);
        nic.set_mac(self.mac);
        self.ptp.configure(nic, role, self.mac, port);
        self.status(nic, reply, Status::Ok);
    }

    fn unblock<N: Nic>(&mut self, nic: &mut N, reply: &ReplyTo) {
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
        if !self.grandmaster {
            return Status::NotGrandmaster;
        }
        let Ok((req, _)) = SetTimeBody::ref_from_prefix(body) else {
            return Status::InvalidPayload;
        };
        nic.stop_pulse();
        if !nic.set_time(req.sys_time.get()) {
            if self.ptp.is_locked() {
                nic.arm_pulse();
            }
            return Status::InvalidPayload;
        }
        self.ptp.time_set();
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
            ptp_offset_ns: I32::new(self.ptp_offset_ns()),
        }
    }

    fn ptp_offset_ns(&self) -> i32 {
        if !self.is_assigned() || self.grandmaster {
            return 0;
        }
        self.ptp
            .last_offset()
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    }

    fn frame_reply<N: Nic>(&mut self, nic: &mut N, reply: &ReplyTo, state: &Reply) {
        let head = FrameReply {
            ack: state.ack,
            status: state.status,
            flags: self.flags(nic),
            unit_id: self.unit_id,
            sys_time: U64::new(nic.now().unwrap_or(0)),
        };
        let data = state.data();
        let mut body = [0u8; FRAME_REPLY_HEAD_BYTES + REPLY_DATA_BYTES_MAX];
        body[..FRAME_REPLY_HEAD_BYTES].copy_from_slice(head.as_bytes());
        body[FRAME_REPLY_HEAD_BYTES..FRAME_REPLY_HEAD_BYTES + data.len()].copy_from_slice(data);
        self.send_reply(
            nic,
            reply,
            None,
            &body[..FRAME_REPLY_HEAD_BYTES + data.len()],
        );
    }

    fn status<N: Nic>(&mut self, nic: &mut N, reply: &ReplyTo, status: Status) {
        self.send_reply(nic, reply, Some(status), &[]);
    }

    fn send_reply<N: Nic>(
        &mut self,
        nic: &mut N,
        reply: &ReplyTo,
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
            nic.send(&self.buf[..len], reply.port, false);
        }
    }
}
