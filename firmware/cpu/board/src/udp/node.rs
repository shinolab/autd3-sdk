use zerocopy::little_endian::{I32, U64};
use zerocopy::{FromBytes, IntoBytes};

use autd3_cpu_fw::net::{
    self, Endpoint, Ipv6, Mac, NDP_HOP_LIMIT, Packet, UDP_PAYLOAD_OFFSET, Udp,
};
use autd3_cpu_fw::proto::{FRAME_BYTES_MAX, FrameHeader, REPLY_DATA_BYTES_MAX, Reply};
use autd3_cpu_fw::udp::{
    ALL_NODES, AssignIdBody, Flags, FrameReply, Header, Kind, PORT, PROTOCOL_VERSION,
    RESET_ID_CLOSE_DELAY_MS, Role, SetTimeBody, Status, UNASSIGNED_ID, UnblockReply, UnitInfo,
    link_local, mac, solicited_node,
};

use super::ptp::Ptp;
use super::{RxMeta, Tx, ethsw, other_port, pulse};

const TX_BUF_BYTES: usize = 320;

const _: () = assert!(
    TX_BUF_BYTES
        >= UDP_PAYLOAD_OFFSET
            + size_of::<Header>()
            + size_of::<FrameReply>()
            + REPLY_DATA_BYTES_MAX
);
const _: () = assert!(
    TX_BUF_BYTES
        >= UDP_PAYLOAD_OFFSET + size_of::<Header>() + size_of::<Status>() + size_of::<UnitInfo>()
);
const _: () = assert!(
    TX_BUF_BYTES
        >= UDP_PAYLOAD_OFFSET
            + size_of::<Header>()
            + size_of::<Status>()
            + size_of::<UnblockReply>()
);

pub(crate) struct Node {
    unit_id: u8,
    upstream: Option<u8>,
    open: bool,
    downstream_link: bool,
    close_at: Option<u32>,
    mac: Mac,
    addr: Ipv6,
    host: Option<(u8, Endpoint)>,
    last_host_frame_ms: Option<u32>,
    ptp: Ptp,
    buf: [u8; TX_BUF_BYTES],
}

struct ReplyTo {
    port: u8,
    to: Endpoint,
    kind: u8,
    msg_id: u16,
}

impl Node {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            unit_id: UNASSIGNED_ID,
            upstream: None,
            open: false,
            downstream_link: false,
            close_at: None,
            mac: mac(UNASSIGNED_ID),
            addr: link_local(mac(UNASSIGNED_ID)),
            host: None,
            last_host_frame_ms: None,
            ptp: Ptp::new(),
            buf: [0; TX_BUF_BYTES],
        }
    }

    pub(crate) fn init(&mut self) {
        self.unassign();
    }

    #[must_use]
    pub(crate) const fn is_assigned(&self) -> bool {
        self.unit_id != UNASSIGNED_ID
    }

    const fn is_grandmaster(&self) -> bool {
        matches!(self.ptp.role(), Some(Role::Grandmaster))
    }

    pub(crate) const fn configure_ptp(&mut self, config: autd3_cpu_fw::ptp::Config) {
        self.ptp.set_config(config);
    }

    pub(crate) fn flags(&self, failsafe_active: bool) -> Flags {
        let mut flags = Flags::empty();
        if self.is_assigned() {
            flags |= Flags::ASSIGNED;
        }
        if self.open {
            flags |= Flags::DOWNSTREAM_OPEN;
        }
        if self.downstream_link {
            flags |= Flags::DOWNSTREAM_LINK;
        }
        if pulse::is_ready() {
            flags |= Flags::SYNC_READY;
        }
        if self.ptp.is_locked() {
            flags |= Flags::PTP_LOCKED;
        }
        if self.is_grandmaster() {
            flags |= Flags::GRANDMASTER;
        }
        if failsafe_active {
            flags |= Flags::FAILSAFE;
        }
        flags
    }

    fn unassign(&mut self) {
        ethsw::set_forwarding(false);
        *self = Self {
            last_host_frame_ms: self.last_host_frame_ms,
            ptp: core::mem::take(&mut self.ptp),
            ..Self::new()
        };
        ethsw::set_own_mac(self.mac);
        self.ptp.reset();
        pulse::stop();
    }

    pub(crate) fn tick(&mut self, tx: &mut Tx, now_ms: u32) {
        if let Some(at) = self.close_at
            && now_ms.wrapping_sub(at).cast_signed() >= 0
        {
            self.unassign();
        }
        self.ptp.tick(tx, now_ms);
        crate::cpu().publish_ptp_unlocked(self.ptp.unlocked_ms(now_ms));
        crate::cpu().publish_host_idle(
            self.last_host_frame_ms
                .map(|last| now_ms.wrapping_sub(last)),
        );
    }

    pub(crate) fn send_completion(&mut self, tx: &mut Tx, msg_id: u16) {
        let Some((port, to)) = self.host.filter(|_| self.is_assigned()) else {
            return;
        };
        let reply_to = ReplyTo {
            port,
            to,
            kind: Kind::Frame.as_u8(),
            msg_id,
        };
        let state = crate::cpu().reply();
        let failsafe_active = crate::cpu().failsafe_active();
        self.frame_reply(tx, &reply_to, &state, failsafe_active);
    }

    pub(crate) fn on_frame(&mut self, tx: &mut Tx, now_ms: u32, frame: &[u8], rx: RxMeta) {
        match net::parse(frame) {
            Some(Packet::Ptp { message }) => {
                if self.is_assigned() {
                    self.ptp.on_message(tx, message, rx, now_ms);
                    crate::cpu().publish_ptp_unlocked(self.ptp.unlocked_ms(now_ms));
                }
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
                    let _ = tx.send(&self.buf[..len], rx.port, false);
                }
            }
            Some(Packet::Udp(udp))
                if udp.dst_port == PORT
                    && (udp.dst_ip == self.addr || udp.dst_ip == ALL_NODES)
                    && net::is_link_local_unicast(&udp.src_ip) =>
            {
                self.on_udp(tx, now_ms, &udp, rx);
            }
            Some(Packet::Udp(_)) | None => {}
        }
    }

    fn on_udp(&mut self, tx: &mut Tx, now_ms: u32, udp: &Udp<'_>, rx: RxMeta) {
        let Ok((header, body)) = Header::ref_from_prefix(udp.payload) else {
            return;
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
            self.status(tx, &reply, Status::UnsupportedVersion);
            return;
        }
        let unicast = udp.dst_ip == self.addr;
        match Kind::from_u8(header.kind) {
            Some(kind @ (Kind::Frame | Kind::Heartbeat)) => {
                self.frame_or_heartbeat(tx, &reply, now_ms, kind, unicast, body);
            }
            Some(Kind::ReadUnitInfo) => self.read_unit_info(tx, &reply),
            Some(Kind::ResetId) => self.reset_id(tx, &reply, now_ms),
            Some(Kind::Discover) => self.discover(tx, &reply),
            Some(Kind::AssignId) if unicast && !self.is_assigned() => {
                self.assign_id(tx, &reply, body);
            }
            Some(Kind::UnblockDownstream) if unicast => self.unblock_downstream(tx, &reply),
            Some(Kind::SetTime) if unicast => self.set_time(tx, &reply, body),
            Some(Kind::AssignId | Kind::UnblockDownstream | Kind::SetTime) | None => {}
            Some(_) => unreachable!(),
        }
    }

    fn frame_or_heartbeat(
        &mut self,
        tx: &mut Tx,
        reply: &ReplyTo,
        now_ms: u32,
        kind: Kind,
        unicast: bool,
        body: &[u8],
    ) {
        let is_frame = matches!(kind, Kind::Frame);
        if !self.is_assigned()
            || (is_frame
                && (!unicast
                    || !(size_of::<FrameHeader>()..=FRAME_BYTES_MAX).contains(&body.len())))
        {
            return;
        }
        self.host = Some((reply.port, reply.to));
        self.last_host_frame_ms = Some(now_ms);
        crate::cpu().publish_host_idle(Some(0));
        if is_frame {
            crate::cpu().recv_frame(body, reply.msg_id);
        } else {
            let state = crate::cpu().reply();
            let failsafe_active = crate::cpu().failsafe_active();
            self.frame_reply(tx, reply, &state, failsafe_active);
        }
    }

    fn read_unit_info(&mut self, tx: &mut Tx, reply: &ReplyTo) {
        #[allow(clippy::cast_possible_truncation)]
        let ptp_offset_ns = if self.is_assigned() && !self.is_grandmaster() {
            self.ptp
                .last_offset()
                .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
        } else {
            0
        };
        let info = UnitInfo {
            unit_id: self.unit_id,
            flags: self.flags(crate::cpu().failsafe_active()),
            ptp_offset_ns: I32::new(ptp_offset_ns),
        };
        self.send_reply(tx, reply, Some(Status::Ok), info.as_bytes());
    }

    fn reset_id(&mut self, tx: &mut Tx, reply: &ReplyTo, now_ms: u32) {
        self.status(tx, reply, Status::Ok);
        if self.close_at.is_none() {
            self.close_at = Some(now_ms.wrapping_add(RESET_ID_CLOSE_DELAY_MS));
        }
    }

    fn discover(&mut self, tx: &mut Tx, reply: &ReplyTo) {
        if !self.is_assigned() {
            self.status(tx, reply, Status::Ok);
        }
    }

    fn assign_id(&mut self, tx: &mut Tx, reply: &ReplyTo, body: &[u8]) {
        let status = if let Ok((req, _)) = AssignIdBody::ref_from_prefix(body)
            && let Some(role) = Role::from_u8(req.role)
            && req.unit_id != UNASSIGNED_ID
        {
            self.unit_id = req.unit_id;
            self.upstream = Some(reply.port);
            self.close_at = None;
            self.mac = mac(req.unit_id);
            self.addr = link_local(self.mac);
            ethsw::set_own_mac(self.mac);
            self.ptp.configure(role, self.mac, reply.port);
            Status::Ok
        } else {
            Status::InvalidPayload
        };
        self.status(tx, reply, status);
    }

    fn unblock_downstream(&mut self, tx: &mut Tx, reply: &ReplyTo) {
        let Some(upstream) = self.upstream.filter(|_| self.is_assigned()) else {
            self.status(tx, reply, Status::NotAssigned);
            return;
        };
        ethsw::set_forwarding(true);
        self.open = true;
        self.downstream_link = super::downstream_link(other_port(upstream));
        let body = UnblockReply {
            downstream_link: u8::from(self.downstream_link),
        };
        self.send_reply(tx, reply, Some(Status::Ok), body.as_bytes());
    }

    fn set_time(&mut self, tx: &mut Tx, reply: &ReplyTo, body: &[u8]) {
        let status = if !self.is_assigned() {
            Status::NotAssigned
        } else if !self.is_grandmaster() {
            Status::NotGrandmaster
        } else if let Ok((req, _)) = SetTimeBody::ref_from_prefix(body) {
            pulse::stop();
            if ethsw::set_time(req.sys_time.get()) {
                self.ptp.time_set();
                self.ptp.arm_pulse();
                Status::Ok
            } else {
                if self.ptp.is_locked() {
                    self.ptp.arm_pulse();
                }
                Status::InvalidPayload
            }
        } else {
            Status::InvalidPayload
        };
        self.status(tx, reply, status);
    }

    fn frame_reply(&mut self, tx: &mut Tx, reply: &ReplyTo, state: &Reply, failsafe_active: bool) {
        let head = FrameReply {
            reserved: 0,
            status: state.status.as_u8(),
            flags: self.flags(failsafe_active),
            unit_id: self.unit_id,
            sys_time: U64::new(ethsw::capture().unwrap_or(0)),
        };
        let data = state.data();
        let mut body = [0u8; size_of::<FrameReply>() + REPLY_DATA_BYTES_MAX];
        body[..size_of::<FrameReply>()].copy_from_slice(head.as_bytes());
        body[size_of::<FrameReply>()..][..data.len()].copy_from_slice(data);
        self.send_reply(
            tx,
            reply,
            None,
            &body[..size_of::<FrameReply>() + data.len()],
        );
    }

    fn status(&mut self, tx: &mut Tx, reply: &ReplyTo, status: Status) {
        self.send_reply(tx, reply, Some(status), &[]);
    }

    fn send_reply(&mut self, tx: &mut Tx, reply: &ReplyTo, status: Option<Status>, body: &[u8]) {
        let header = Header {
            version: PROTOCOL_VERSION,
            kind: reply.kind,
            msg_id: zerocopy::little_endian::U16::new(reply.msg_id),
        };
        let mut at = UDP_PAYLOAD_OFFSET;
        self.buf[at..][..size_of::<Header>()].copy_from_slice(header.as_bytes());
        at += size_of::<Header>();
        if let Some(status) = status {
            self.buf[at] = status.as_u8();
            at += 1;
        }
        self.buf[at..][..body.len()].copy_from_slice(body);
        at += body.len();
        let src = Endpoint {
            mac: self.mac,
            ip: self.addr,
            port: PORT,
        };
        if let Some(len) = net::udp(&mut self.buf, &src, &reply.to, at - UDP_PAYLOAD_OFFSET) {
            let _ = tx.send(&self.buf[..len], reply.port, false);
        }
    }
}
