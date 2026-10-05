use zerocopy::little_endian::{I32, U64};
use zerocopy::{FromBytes, IntoBytes};

use autd3_cpu_wire::udp::{
    ALL_NODES, AssignIdBody, Flags, FrameReply, Header, Kind, PORT, PROTOCOL_VERSION,
    RESET_ID_CLOSE_DELAY_MS, Role, SetTimeBody, Status, UNASSIGNED_ID, UPSTREAM_UNKNOWN,
    UnblockReply, UnitInfo, link_local, mac, solicited_node,
};

use crate::net::{self, Endpoint, Ipv6, Mac, NDP_HOP_LIMIT, Packet, UDP_PAYLOAD_OFFSET, Udp};
use crate::nic::{Nic, RxMeta, other_port};
use crate::proto::{Disposition, FRAME_BYTES_MAX, FrameHeader, REPLY_DATA_BYTES_MAX, Reply};
use crate::ptp::Ptp;
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

impl CommandLayer for &crate::Cpu {
    fn recv_frame(&mut self, frame: &[u8], msg_id: u16) -> Disposition {
        crate::Cpu::recv_frame(self, frame, msg_id)
    }

    fn reply(&mut self) -> Reply {
        crate::Cpu::reply(self)
    }
}

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

pub struct Node {
    unit_id: u8,
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
    pub const fn is_assigned(&self) -> bool {
        self.unit_id != UNASSIGNED_ID
    }

    const fn is_grandmaster(&self) -> bool {
        matches!(self.ptp.role(), Some(Role::Grandmaster))
    }

    pub const fn configure_ptp(&mut self, config: autd3_cpu_wire::config::PtpConfig) {
        self.ptp.set_config(config);
    }

    #[must_use]
    pub const fn ptp_unlocked_ms(&self, now_ms: u32) -> Option<u32> {
        self.ptp.unlocked_ms(now_ms)
    }

    pub fn flags<N: Nic>(&self, nic: &mut N) -> Flags {
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
        if nic.pulse_ready() {
            flags |= Flags::SYNC_READY;
        }
        if self.ptp.is_locked() {
            flags |= Flags::PTP_LOCKED;
        }
        if self.is_grandmaster() {
            flags |= Flags::GRANDMASTER;
        }
        flags
    }

    fn unassign<N: Nic>(&mut self, nic: &mut N) {
        nic.set_forwarding(false);
        let ptp_config = self.ptp.config();
        *self = Self::new();
        self.ptp.set_config(ptp_config);
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
        self.ptp.tick(nic, now_ms);
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
            Some(Packet::Ptp { message }) => {
                if self.is_assigned() {
                    self.ptp.on_message(nic, message, rx, now_ms);
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
            Some(kind @ (Kind::Frame | Kind::Heartbeat)) => {
                let is_frame = matches!(kind, Kind::Frame);
                if !self.is_assigned()
                    || !unicast
                    || (is_frame
                        && !(size_of::<FrameHeader>()..=FRAME_BYTES_MAX).contains(&body.len()))
                {
                    return Received::Nothing;
                }
                self.host = Some((reply.port, reply.to));
                if !is_frame || cmds.recv_frame(body, reply.msg_id) == Disposition::Reply {
                    let state = cmds.reply();
                    self.frame_reply(nic, &reply, &state);
                }
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
                let status = self.assign(nic, body, reply.port);
                self.status(nic, &reply, status);
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

    fn assign<N: Nic>(&mut self, nic: &mut N, body: &[u8], upstream: u8) -> Status {
        let Ok((req, _)) = AssignIdBody::ref_from_prefix(body) else {
            return Status::InvalidPayload;
        };
        let Some(role) = Role::from_u8(req.role) else {
            return Status::InvalidPayload;
        };
        if req.unit_id == UNASSIGNED_ID {
            return Status::InvalidPayload;
        }
        self.unit_id = req.unit_id;
        self.upstream = Some(upstream);
        self.close_at = None;
        self.mac = mac(req.unit_id);
        self.addr = link_local(self.mac);
        nic.set_mac(self.mac);
        self.ptp.configure(nic, role, self.mac, upstream);
        Status::Ok
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
        if !self.is_grandmaster() {
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
        if !self.is_assigned() || self.is_grandmaster() {
            return 0;
        }
        self.ptp
            .last_offset()
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    }

    fn frame_reply<N: Nic>(&mut self, nic: &mut N, reply: &ReplyTo, state: &Reply) {
        let head = FrameReply {
            ack: state.ack,
            status: state.status.as_u8(),
            flags: self.flags(nic),
            unit_id: self.unit_id,
            sys_time: U64::new(nic.now().unwrap_or(0)),
        };
        let data = state.data();
        let mut body = [0u8; size_of::<FrameReply>() + REPLY_DATA_BYTES_MAX];
        body[..size_of::<FrameReply>()].copy_from_slice(head.as_bytes());
        body[size_of::<FrameReply>()..][..data.len()].copy_from_slice(data);
        self.send_reply(
            nic,
            reply,
            None,
            &body[..size_of::<FrameReply>() + data.len()],
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
        self.buf[at..][..size_of::<Header>()].copy_from_slice(header.as_bytes());
        at += size_of::<Header>();
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

#[cfg(test)]
mod tests {
    use std::vec;
    use std::vec::Vec;

    use autd3_cpu_wire::udp::{
        ALL_NODES, AssignIdBody, Flags, FrameReply, Header, Kind, PORT, PROTOCOL_VERSION, Role,
        SetTimeBody, Status, UNASSIGNED_ID, UnblockReply, UnitInfo, mac, solicited_node,
        unit_address,
    };
    use etherparse::{EtherType, IpNumber, icmpv6};
    use zerocopy::little_endian::U64;
    use zerocopy::{FromBytes, IntoBytes};

    use super::{CommandLayer, Node, Received};
    use crate::net::{
        self, ETH_HEADER, ETHERTYPE_PTP, Endpoint, IPV6_HEADER, Ipv6, MAX_FRAME, Mac, Packet,
        UDP_PAYLOAD_OFFSET,
    };
    use crate::nic::{NS_PER_SEC, Nic, RxMeta, other_port};
    use crate::proto::{Disposition, Error, FRAME_BYTES_MAX, Reply as CmdReply, ReplyData};
    use crate::sim_nic::{SimClock, SimNic};

    const HOST_MAC: Mac = [0x3C, 0x7C, 0x3F, 0x11, 0x22, 0x33];
    const HOST_IP: Ipv6 = [
        0xFE, 0x80, 0, 0, 0, 0, 0, 0, 0x3E, 0x7C, 0x3F, 0xFF, 0xFE, 0x11, 0x22, 0x33,
    ];
    const HOST_PORT: u16 = 51234;
    const UPSTREAM: u8 = 1;

    struct FakeCommands {
        frames: Vec<(Vec<u8>, u16)>,
        disposition: Disposition,
        reply: CmdReply,
    }

    impl CommandLayer for FakeCommands {
        fn recv_frame(&mut self, frame: &[u8], msg_id: u16) -> Disposition {
            self.frames.push((frame.to_vec(), msg_id));
            if self.disposition == Disposition::Reply {
                self.reply =
                    CmdReply::new(frame[0], Error::None, ReplyData::from_slice(&[0x5A, 0x5B]));
            }
            self.disposition
        }

        fn reply(&mut self) -> CmdReply {
            self.reply
        }
    }

    struct Harness {
        node: Node,
        nic: SimNic,
        now_ms: u32,
        cmds: FakeCommands,
    }

    struct Reply {
        port: u8,
        src_mac: Mac,
        src_ip: Ipv6,
        dst_ip: Ipv6,
        dst_port: u16,
        header: Header,
        body: Vec<u8>,
    }

    impl Harness {
        fn new() -> Self {
            Self::with_clock(SimClock::new(5_000_000_000.0, 0.0))
        }

        fn with_clock(clock: SimClock) -> Self {
            let mut nic = SimNic::with_clock(clock);
            let mut node = Node::new();
            node.init(&mut nic);
            nic.sent.clear();
            Self {
                node,
                nic,
                now_ms: 0,
                cmds: FakeCommands {
                    frames: Vec::new(),
                    disposition: Disposition::Reply,
                    reply: CmdReply::new(0xFF, Error::None, ReplyData::EMPTY),
                },
            }
        }

        fn request(
            &mut self,
            dst: Ipv6,
            version: u8,
            kind: u8,
            msg_id: u16,
            body: &[u8],
        ) -> Received {
            let mut buf = vec![0u8; MAX_FRAME];
            let header = Header {
                version,
                kind,
                msg_id: zerocopy::little_endian::U16::new(msg_id),
            };
            buf[UDP_PAYLOAD_OFFSET..][..size_of::<Header>()].copy_from_slice(header.as_bytes());
            buf[UDP_PAYLOAD_OFFSET + size_of::<Header>()..][..body.len()].copy_from_slice(body);
            let src = Endpoint {
                mac: HOST_MAC,
                ip: HOST_IP,
                port: HOST_PORT,
            };
            let to = Endpoint {
                mac: [0; 6],
                ip: dst,
                port: PORT,
            };
            let len = net::udp(&mut buf, &src, &to, size_of::<Header>() + body.len()).unwrap();
            buf[0..6].copy_from_slice(&[0xAA; 6]);
            self.deliver(&buf[..len])
        }

        fn deliver(&mut self, frame: &[u8]) -> Received {
            let rx = RxMeta {
                port: UPSTREAM,
                timestamp_ns: 0,
            };
            self.deliver_at(frame, rx)
        }

        fn deliver_at(&mut self, frame: &[u8], rx: RxMeta) -> Received {
            self.node
                .on_frame(&mut self.nic, self.now_ms, frame, rx, &mut self.cmds)
        }

        fn send(&mut self, dst: Ipv6, kind: Kind, body: &[u8]) -> Received {
            self.request(dst, PROTOCOL_VERSION, kind.as_u8(), 0x4242, body)
        }

        fn replies(&mut self) -> Vec<Reply> {
            self.nic
                .sent
                .drain(..)
                .filter_map(|s| {
                    let Some(Packet::Udp(u)) = net::parse(&s.frame) else {
                        return None;
                    };
                    let (header, body) = Header::read_from_prefix(u.payload).unwrap();
                    Some(Reply {
                        port: s.port,
                        src_mac: u.src_mac,
                        src_ip: u.src_ip,
                        dst_ip: u.dst_ip,
                        dst_port: u.dst_port,
                        header,
                        body: body.to_vec(),
                    })
                })
                .collect()
        }

        fn only_reply(&mut self) -> Reply {
            let mut r = self.replies();
            assert_eq!(r.len(), 1);
            r.pop().unwrap()
        }

        fn unit_info(&mut self, dst: Ipv6) -> UnitInfo {
            self.send(dst, Kind::ReadUnitInfo, &[]);
            let r = self.only_reply();
            assert_eq!(r.body[0], Status::Ok.as_u8());
            UnitInfo::read_from_bytes(&r.body[1..]).unwrap()
        }

        fn assign(&mut self, id: u8) {
            let body = AssignIdBody {
                unit_id: id,
                role: role_of(id),
            };
            self.send(unit_address(UNASSIGNED_ID), Kind::AssignId, body.as_bytes());
            let r = self.only_reply();
            assert_eq!(r.body, [Status::Ok.as_u8()]);
        }

        fn tick(&mut self, ms: u32) {
            for _ in 0..ms {
                self.now_ms += 1;
                self.node.tick(&mut self.nic, self.now_ms);
            }
        }
    }

    fn role_of(id: u8) -> u8 {
        if id == 0 {
            Role::Grandmaster.as_u8()
        } else {
            Role::Slave.as_u8()
        }
    }

    fn frame_body(seq: u8) -> Vec<u8> {
        let mut f = vec![0u8; FRAME_BYTES_MAX];
        f[0] = seq;
        f[1] = 0x04;
        f[FRAME_BYTES_MAX - 1] = 0xEE;
        f
    }

    fn parse_frame_reply(body: &[u8]) -> (FrameReply, Vec<u8>) {
        let (head, data) = FrameReply::read_from_prefix(body).unwrap();
        (head, data.to_vec())
    }

    #[test]
    fn an_unassigned_unit_answers_read_unit_info_and_discover() {
        let mut h = Harness::new();
        let info = h.unit_info(ALL_NODES);
        assert_eq!(info.unit_id, UNASSIGNED_ID);
        assert_eq!(info.flags, Flags::empty());
        assert_eq!(info.upstream_port, 0xFF);
        assert_eq!(info.sys_time.get(), 5_000_000_000);

        h.send(ALL_NODES, Kind::Discover, &[]);
        let r = h.only_reply();
        assert_eq!(r.port, UPSTREAM);
        assert_eq!(r.src_ip, unit_address(UNASSIGNED_ID));
        assert_eq!(r.src_mac, mac(UNASSIGNED_ID));
        assert_eq!(r.dst_ip, HOST_IP);
        assert_eq!(r.dst_port, HOST_PORT);
        assert_eq!(r.header, Header::new(Kind::Discover, 0x4242));
        assert_eq!(r.body, [Status::Ok.as_u8()]);
    }

    #[test]
    fn assign_id_is_accepted_only_by_unicast_and_moves_the_address() {
        let mut h = Harness::new();
        let body = AssignIdBody {
            unit_id: 3,
            role: Role::Slave.as_u8(),
        };
        h.send(ALL_NODES, Kind::AssignId, body.as_bytes());
        assert!(h.replies().is_empty());
        assert!(!h.node.is_assigned());

        h.send(unit_address(UNASSIGNED_ID), Kind::AssignId, body.as_bytes());
        let r = h.only_reply();
        assert_eq!(r.body, [Status::Ok.as_u8()]);
        assert_eq!(r.src_ip, unit_address(3));
        assert_eq!(r.src_mac, mac(3));
        assert_eq!(h.nic.mac, Some(mac(3)));
        assert_eq!(h.node.unit_id, 3);

        h.send(ALL_NODES, Kind::Discover, &[]);
        assert!(h.replies().is_empty());
        h.send(unit_address(3), Kind::AssignId, body.as_bytes());
        assert!(h.replies().is_empty());
        h.send(unit_address(UNASSIGNED_ID), Kind::ReadUnitInfo, &[]);
        assert!(h.replies().is_empty());

        let info = h.unit_info(unit_address(3));
        assert_eq!(info.unit_id, 3);
        assert_eq!(info.flags, Flags::ASSIGNED);
        assert_eq!(info.upstream_port, UPSTREAM);
        assert_eq!(
            info.fw_version,
            [
                crate::FW_VERSION_MAJOR,
                crate::FW_VERSION_MINOR,
                crate::FW_VERSION_PATCH
            ]
        );
    }

    #[test]
    fn the_reserved_id_is_refused() {
        let mut h = Harness::new();
        let body = AssignIdBody {
            unit_id: UNASSIGNED_ID,
            role: Role::Slave.as_u8(),
        };
        h.send(unit_address(UNASSIGNED_ID), Kind::AssignId, body.as_bytes());
        assert_eq!(h.only_reply().body, [Status::InvalidPayload.as_u8()]);
        let body = AssignIdBody {
            unit_id: 1,
            role: 2,
        };
        h.send(unit_address(UNASSIGNED_ID), Kind::AssignId, body.as_bytes());
        assert_eq!(h.only_reply().body, [Status::InvalidPayload.as_u8()]);
        h.send(unit_address(UNASSIGNED_ID), Kind::AssignId, &[1]);
        assert_eq!(h.only_reply().body, [Status::InvalidPayload.as_u8()]);
        h.send(unit_address(UNASSIGNED_ID), Kind::AssignId, &[]);
        assert_eq!(h.only_reply().body, [Status::InvalidPayload.as_u8()]);
        assert!(!h.node.is_assigned());
    }

    #[test]
    fn unblock_opens_the_downstream_and_reports_its_link() {
        let mut h = Harness::new();
        h.send(unit_address(UNASSIGNED_ID), Kind::UnblockDownstream, &[]);
        assert_eq!(h.only_reply().body, [Status::NotAssigned.as_u8()]);

        h.assign(0);
        h.nic.link = true;
        h.send(ALL_NODES, Kind::UnblockDownstream, &[]);
        assert!(h.replies().is_empty());
        assert_eq!(h.nic.forwarding_open, Some(false));

        h.send(unit_address(0), Kind::UnblockDownstream, &[]);
        let r = h.only_reply();
        assert_eq!(r.body[0], Status::Ok.as_u8());
        assert_eq!(
            UnblockReply::read_from_bytes(&r.body[1..]).unwrap(),
            UnblockReply { downstream_link: 1 }
        );
        assert_eq!(h.nic.forwarding_open, Some(true));
        let info = h.unit_info(unit_address(0));
        assert_eq!(
            info.flags,
            Flags::ASSIGNED | Flags::DOWNSTREAM_OPEN | Flags::DOWNSTREAM_LINK | Flags::GRANDMASTER
        );
    }

    #[test]
    fn set_time_needs_an_assigned_unit() {
        let mut h = Harness::new();
        let t = SetTimeBody {
            sys_time: U64::new(812_000_000_123_456_789),
        };
        h.send(unit_address(UNASSIGNED_ID), Kind::SetTime, t.as_bytes());
        assert_eq!(h.only_reply().body, [Status::NotAssigned.as_u8()]);
        assert_eq!(h.nic.time_set, None);
        assert_eq!(h.nic.pulse_armed, 0);
    }

    #[test]
    fn the_grandmaster_takes_the_host_time_locks_and_arms_its_pulse() {
        let mut h = Harness::new();
        h.assign(0);
        assert_eq!(
            h.unit_info(unit_address(0)).flags,
            Flags::ASSIGNED | Flags::GRANDMASTER
        );
        let t = SetTimeBody {
            sys_time: U64::new(812_000_000_123_456_789),
        };
        h.send(unit_address(0), Kind::SetTime, &t.as_bytes()[..4]);
        assert_eq!(h.only_reply().body, [Status::InvalidPayload.as_u8()]);
        h.send(ALL_NODES, Kind::SetTime, t.as_bytes());
        assert!(h.replies().is_empty());
        assert_eq!(h.nic.time_set, None);

        h.send(unit_address(0), Kind::SetTime, t.as_bytes());
        assert_eq!(h.only_reply().body, [Status::Ok.as_u8()]);
        assert_eq!(h.nic.time_set, Some(812_000_000_123_456_789));
        assert_eq!(h.nic.pulse_armed, 1);
        assert!(h.node.ptp.is_locked());
        assert_eq!(
            h.unit_info(unit_address(0)).flags,
            Flags::ASSIGNED | Flags::GRANDMASTER | Flags::PTP_LOCKED
        );

        h.nic.pulse_ready = true;
        let info = h.unit_info(unit_address(0));
        assert_eq!(
            info.flags,
            Flags::ASSIGNED | Flags::GRANDMASTER | Flags::PTP_LOCKED | Flags::SYNC_READY
        );
        assert_eq!(info.sys_time.get(), 812_000_000_123_456_789);
        assert_eq!(info.ptp_offset_ns.get(), 0);
    }

    #[test]
    fn a_slave_refuses_set_time_and_waits_for_ptp() {
        let mut h = Harness::new();
        h.assign(2);
        let t = SetTimeBody {
            sys_time: U64::new(812_000_000_123_456_789),
        };
        h.send(unit_address(2), Kind::SetTime, t.as_bytes());
        assert_eq!(h.only_reply().body, [Status::NotGrandmaster.as_u8()]);
        assert_eq!(h.nic.time_set, None);
        assert_eq!(h.nic.pulse_armed, 0);
        assert!(!h.node.ptp.is_locked());
        assert_eq!(h.unit_info(unit_address(2)).flags, Flags::ASSIGNED);
    }

    #[test]
    fn a_frame_reaches_the_command_layer_and_is_answered_at_once() {
        let mut h = Harness::new();
        let body = frame_body(7);
        assert_eq!(
            h.send(unit_address(UNASSIGNED_ID), Kind::Frame, &body),
            Received::Nothing
        );
        assert_eq!(h.cmds.frames, []);
        assert!(h.replies().is_empty());

        h.assign(1);
        assert_eq!(h.send(ALL_NODES, Kind::Frame, &body), Received::Nothing);
        assert!(h.replies().is_empty());
        assert_eq!(
            h.send(unit_address(1), Kind::Frame, &body[..1]),
            Received::Nothing
        );
        assert!(h.replies().is_empty());

        assert_eq!(
            h.send(unit_address(1), Kind::Frame, &body),
            Received::HostMessage
        );
        assert_eq!(h.cmds.frames.len(), 1);
        assert_eq!(h.cmds.frames[0].0, body);
        assert_eq!(h.cmds.frames[0].1, 0x4242);
        let r = h.only_reply();
        assert_eq!(r.header, Header::new(Kind::Frame, 0x4242));
        let (reply, data) = parse_frame_reply(&r.body);
        assert_eq!(reply.ack, 7);
        assert_eq!(reply.status, 0);
        assert_eq!(data, [0x5A, 0x5B]);
        assert_eq!(reply.unit_id, 1);
        assert_eq!(reply.flags, Flags::ASSIGNED);
        assert_eq!(reply.sys_time.get(), 5_000_000_000);
    }

    #[test]
    fn a_short_frame_is_passed_on_without_padding() {
        let mut h = Harness::new();
        h.assign(1);
        let body = [9, 0x04, 1, 2];
        assert_eq!(
            h.send(unit_address(1), Kind::Frame, &body),
            Received::HostMessage
        );
        assert_eq!(h.cmds.frames[0].0, body);
        let (reply, _) = parse_frame_reply(&h.only_reply().body);
        assert_eq!(reply.ack, 9);
    }

    #[test]
    fn a_deferred_frame_is_answered_by_the_completion() {
        let mut h = Harness::new();
        h.assign(2);
        h.cmds.disposition = Disposition::Deferred;
        assert_eq!(
            h.request(
                unit_address(2),
                PROTOCOL_VERSION,
                Kind::Frame.as_u8(),
                0x0101,
                &frame_body(3)
            ),
            Received::HostMessage
        );
        assert!(h.replies().is_empty());

        h.node.send_completion(
            &mut h.nic,
            0x0101,
            &CmdReply::new(3, Error::InvalidPayload, ReplyData::EMPTY),
        );
        let r = h.only_reply();
        assert_eq!(r.header, Header::new(Kind::Frame, 0x0101));
        assert_eq!(r.port, UPSTREAM);
        assert_eq!(r.dst_ip, HOST_IP);
        assert_eq!(r.dst_port, HOST_PORT);
        assert_eq!(r.src_ip, unit_address(2));
        let (reply, data) = parse_frame_reply(&r.body);
        assert_eq!(reply.ack, 3);
        assert_eq!(reply.status, 0x02);
        assert_eq!(data, []);
    }

    #[test]
    fn a_dropped_frame_gets_no_reply() {
        let mut h = Harness::new();
        h.assign(0);
        h.cmds.disposition = Disposition::Dropped;
        assert_eq!(
            h.send(unit_address(0), Kind::Frame, &frame_body(1)),
            Received::HostMessage
        );
        assert!(h.replies().is_empty());
    }

    #[test]
    fn a_completion_needs_an_assigned_unit_and_a_host() {
        let mut h = Harness::new();
        h.node.send_completion(
            &mut h.nic,
            1,
            &CmdReply::new(0, Error::None, ReplyData::EMPTY),
        );
        assert!(h.replies().is_empty());

        h.assign(0);
        h.node.send_completion(
            &mut h.nic,
            1,
            &CmdReply::new(0, Error::None, ReplyData::EMPTY),
        );
        assert!(h.replies().is_empty());
    }

    #[test]
    fn a_heartbeat_is_answered_with_the_current_result() {
        let mut h = Harness::new();
        assert_eq!(
            h.send(unit_address(UNASSIGNED_ID), Kind::Heartbeat, &[]),
            Received::Nothing
        );
        assert!(h.replies().is_empty());

        h.assign(4);
        assert_eq!(h.send(ALL_NODES, Kind::Heartbeat, &[]), Received::Nothing);
        assert!(h.replies().is_empty());

        h.cmds.reply = CmdReply::new(0x33, Error::None, ReplyData::from_slice(&[1, 2, 3]));
        assert_eq!(
            h.send(unit_address(4), Kind::Heartbeat, &[]),
            Received::HostMessage
        );
        assert_eq!(h.cmds.frames, []);
        let r = h.only_reply();
        assert_eq!(r.header, Header::new(Kind::Heartbeat, 0x4242));
        let (reply, data) = parse_frame_reply(&r.body);
        assert_eq!(reply.ack, 0x33);
        assert_eq!(data, [1, 2, 3]);
        assert_eq!(reply.unit_id, 4);
    }

    #[test]
    fn reset_id_answers_then_closes_after_the_delay() {
        let mut h = Harness::new();
        h.assign(0);
        h.send(unit_address(0), Kind::UnblockDownstream, &[]);
        let _ = h.replies();
        let t = SetTimeBody {
            sys_time: U64::new(812_000_000_000_000_000),
        };
        h.send(unit_address(0), Kind::SetTime, t.as_bytes());
        let _ = h.replies();
        h.nic.pulse_ready = true;

        h.send(ALL_NODES, Kind::ResetId, &[]);
        let r = h.only_reply();
        assert_eq!(r.src_ip, unit_address(0));
        assert_eq!(r.body, [Status::Ok.as_u8()]);
        h.send(ALL_NODES, Kind::ResetId, &[]);
        let _ = h.replies();

        h.tick(19);
        assert!(h.node.is_assigned());
        assert_eq!(h.nic.forwarding_open, Some(true));
        h.tick(1);
        assert!(!h.node.is_assigned());
        assert_eq!(h.nic.forwarding_open, Some(false));
        assert_eq!(h.nic.mac, Some(mac(UNASSIGNED_ID)));
        assert!(!h.nic.pulse_ready);
        let _ = h.replies();
        let info = h.unit_info(ALL_NODES);
        assert_eq!(info.unit_id, UNASSIGNED_ID);
        assert_eq!(info.flags, Flags::empty());
    }

    #[test]
    fn a_foreign_version_gets_only_a_status() {
        let mut h = Harness::new();
        h.request(
            ALL_NODES,
            PROTOCOL_VERSION + 1,
            Kind::ReadUnitInfo.as_u8(),
            9,
            &[],
        );
        let r = h.only_reply();
        assert_eq!(r.header.version, PROTOCOL_VERSION);
        assert_eq!(r.header.kind, Kind::ReadUnitInfo.as_u8());
        assert_eq!(r.header.msg_id.get(), 9);
        assert_eq!(r.body, [Status::UnsupportedVersion.as_u8()]);
    }

    #[test]
    fn unknown_kinds_and_other_destinations_are_dropped() {
        let mut h = Harness::new();
        h.request(ALL_NODES, PROTOCOL_VERSION, 0x7F, 1, &[]);
        assert!(h.replies().is_empty());
        h.send(unit_address(9), Kind::ReadUnitInfo, &[]);
        assert!(h.replies().is_empty());
        h.request(
            ALL_NODES,
            PROTOCOL_VERSION,
            Kind::ReadUnitInfo.as_u8(),
            1,
            &[],
        );
        assert_eq!(h.replies().len(), 1);
    }

    fn icmp_frame(dst: Ipv6, hop_limit: u8, icmp: &[u8]) -> Vec<u8> {
        let mut frame = vec![0u8; ETH_HEADER + IPV6_HEADER + icmp.len()];
        frame[6..12].copy_from_slice(&HOST_MAC);
        frame[12..14].copy_from_slice(&EtherType::IPV6.0.to_be_bytes());
        let ip = &mut frame[ETH_HEADER..];
        ip[0] = 0x60;
        ip[4..6].copy_from_slice(&(icmp.len() as u16).to_be_bytes());
        ip[6] = IpNumber::IPV6_ICMP.0;
        ip[7] = hop_limit;
        ip[8..24].copy_from_slice(&HOST_IP);
        ip[24..40].copy_from_slice(&dst);
        ip[IPV6_HEADER..].copy_from_slice(icmp);
        frame
    }

    fn solicitation(target: Ipv6) -> Vec<u8> {
        let mut icmp = vec![0u8; 32];
        icmp[0] = icmpv6::TYPE_NEIGHBOR_SOLICITATION;
        icmp[8..24].copy_from_slice(&target);
        icmp
    }

    #[test]
    fn a_neighbor_solicitation_for_the_own_address_is_answered() {
        let mut h = Harness::new();
        h.assign(4);
        let own = unit_address(4);
        h.deliver(&icmp_frame(solicited_node(own), 255, &solicitation(own)));
        assert_eq!(h.nic.sent.len(), 1);
        let sent = h.nic.sent.pop().unwrap();
        assert_eq!(sent.port, UPSTREAM);
        assert_eq!(&sent.frame[0..6], &HOST_MAC);
        assert_eq!(&sent.frame[6..12], &mac(4));
        assert_eq!(sent.frame[ETH_HEADER + IPV6_HEADER], 136);

        h.deliver(&icmp_frame(solicited_node(own), 64, &solicitation(own)));
        h.deliver(&icmp_frame(
            solicited_node(unit_address(5)),
            255,
            &solicitation(unit_address(5)),
        ));
        h.deliver(&icmp_frame(own, 255, &solicitation(unit_address(5))));
        assert!(h.nic.sent.is_empty());
        h.deliver(&icmp_frame(own, 255, &solicitation(own)));
        assert_eq!(h.nic.sent.len(), 1);
    }

    #[test]
    fn echo_is_answered_only_on_unicast() {
        let mut h = Harness::new();
        let own = unit_address(UNASSIGNED_ID);
        let mut icmp = vec![icmpv6::TYPE_ECHO_REQUEST, 0, 0, 0, 0, 1, 0, 2, b'x'];
        h.deliver(&icmp_frame(ALL_NODES, 64, &icmp));
        assert!(h.nic.sent.is_empty());
        h.deliver(&icmp_frame(own, 64, &icmp));
        assert_eq!(h.nic.sent.len(), 1);
        let sent = h.nic.sent.pop().unwrap();
        assert_eq!(sent.frame[ETH_HEADER + IPV6_HEADER], 129);
        icmp.resize(400, 0);
        h.deliver(&icmp_frame(own, 64, &icmp));
        assert!(h.nic.sent.is_empty());
    }

    const WIRE_DELAY_NS: f64 = 700.0;
    const HOP_NS: f64 = 10_000.0;
    const HOST_TIME: u64 = 812_000_000_123_456_789;

    struct Wired {
        gm: Harness,
        slave: Harness,
    }

    fn is_ptp(frame: &[u8]) -> bool {
        frame.get(12..14) == Some(&ETHERTYPE_PTP.to_be_bytes()[..])
    }

    impl Wired {
        fn new(slave_crystal_ppb: f64) -> Self {
            let mut gm = Harness::new();
            let mut slave = Harness::with_clock(SimClock::new(1_000_000.0, slave_crystal_ppb));
            gm.assign(0);
            slave.assign(1);
            let t = SetTimeBody {
                sys_time: U64::new(HOST_TIME),
            };
            gm.send(unit_address(0), Kind::SetTime, t.as_bytes());
            assert_eq!(gm.only_reply().body, [Status::Ok.as_u8()]);
            Self { gm, slave }
        }

        fn deliver(&mut self) {
            loop {
                let from_gm = core::mem::take(&mut self.gm.nic.sent);
                let from_slave = core::mem::take(&mut self.slave.nic.sent);
                if from_gm.is_empty() && from_slave.is_empty() {
                    return;
                }
                self.gm.nic.t += HOP_NS;
                self.slave.nic.t = self.gm.nic.t;
                for f in from_gm {
                    if f.port != other_port(UPSTREAM) || !is_ptp(&f.frame) {
                        continue;
                    }
                    let rx = RxMeta {
                        port: UPSTREAM,
                        timestamp_ns: (self.slave.nic.local_at(f.at + WIRE_DELAY_NS) % NS_PER_SEC)
                            as u32,
                    };
                    self.slave.deliver_at(&f.frame, rx);
                }
                for f in from_slave {
                    if f.port != UPSTREAM || !is_ptp(&f.frame) {
                        continue;
                    }
                    let rx = RxMeta {
                        port: other_port(UPSTREAM),
                        timestamp_ns: (self.gm.nic.local_at(f.at + WIRE_DELAY_NS) % NS_PER_SEC)
                            as u32,
                    };
                    self.gm.deliver_at(&f.frame, rx);
                }
            }
        }

        fn run_ms(&mut self, ms: u32) {
            for _ in 0..ms {
                self.gm.nic.t += 1e6;
                self.slave.nic.t = self.gm.nic.t;
                self.gm.tick(1);
                self.slave.tick(1);
                self.deliver();
            }
        }

        fn true_offset(&self) -> f64 {
            (self.slave.nic.clock.at(self.slave.nic.t) - self.gm.nic.clock.at(self.gm.nic.t)) as f64
        }
    }

    #[test]
    fn the_grandmaster_sends_timestamped_syncs_downstream_only_after_set_time() {
        let mut h = Harness::new();
        h.assign(0);
        h.tick(100);
        assert!(h.nic.sent.is_empty());
        let t = SetTimeBody {
            sys_time: U64::new(HOST_TIME),
        };
        h.send(unit_address(0), Kind::SetTime, t.as_bytes());
        h.replies();
        h.tick(1);
        assert_eq!(h.nic.sent.len(), 1);
        let sync = &h.nic.sent[0];
        assert!(is_ptp(&sync.frame));
        assert_eq!(sync.port, other_port(UPSTREAM));
        assert!(sync.timestamp);
        h.tick(1);
        assert_eq!(h.nic.sent.len(), 2);
        assert!(!h.nic.sent[1].timestamp);
    }

    #[test]
    fn an_unassigned_unit_ignores_ptp() {
        let mut wired = Wired::new(0.0);
        wired.run_ms(20);
        let mut idle = Harness::new();
        let frames: Vec<Vec<u8>> = wired.gm.nic.sent.iter().map(|s| s.frame.clone()).collect();
        wired.run_ms(20);
        for f in wired
            .gm
            .nic
            .sent
            .iter()
            .map(|s| s.frame.clone())
            .chain(frames)
        {
            idle.deliver(&f);
        }
        assert!(idle.nic.sent.is_empty());
        assert_eq!(idle.nic.steps, 0);
    }

    #[test]
    fn a_slave_locks_to_the_grandmaster_and_then_arms_its_pulse() {
        let mut wired = Wired::new(-60_000.0);
        assert_eq!(wired.slave.nic.pulse_armed, 0);
        wired.run_ms(5000);
        assert!(wired.slave.node.ptp.is_locked());
        assert_eq!(wired.slave.nic.pulse_armed, 1);
        assert!(wired.true_offset().abs() < 100.0, "{}", wired.true_offset());
        let info = wired.slave.unit_info(unit_address(1));
        assert!(info.flags.contains(Flags::PTP_LOCKED));
        assert!(!info.flags.contains(Flags::GRANDMASTER));
        assert!(info.ptp_offset_ns.get().abs() < 100);
        let now = wired.slave.nic.now().unwrap();
        assert!(now.abs_diff(wired.gm.nic.now().unwrap()) < 100);
        assert!(now >= HOST_TIME);
    }

    #[test]
    fn a_step_unlocks_the_slave_and_stops_its_pulse_until_it_locks_again() {
        let mut wired = Wired::new(25_000.0);
        wired.run_ms(5000);
        assert!(wired.slave.node.ptp.is_locked());
        wired.slave.nic.pulse_ready = true;
        wired.slave.nic.step(200_000);
        wired.run_ms(100);
        assert!(!wired.slave.node.ptp.is_locked());
        assert!(wired.slave.nic.pulse_stopped >= 1);
        assert!(!wired.slave.nic.pulse_ready);
        assert!(
            !wired
                .slave
                .unit_info(unit_address(1))
                .flags
                .intersects(Flags::PTP_LOCKED | Flags::SYNC_READY)
        );
        wired.run_ms(5000);
        assert!(wired.slave.node.ptp.is_locked());
        assert_eq!(wired.slave.nic.pulse_armed, 2);
        assert!(wired.true_offset().abs() < 100.0, "{}", wired.true_offset());
    }

    #[test]
    fn a_locked_slave_stops_its_pulse_before_it_steps() {
        let mut wired = Wired::new(25_000.0);
        wired.run_ms(5000);
        assert!(wired.slave.node.ptp.is_locked());
        let stepped = wired.slave.nic.steps;
        wired.slave.nic.jump(3_000_000);
        wired.run_ms(100);
        assert!(wired.slave.nic.steps > stepped);
        assert_eq!(wired.slave.nic.steps_with_pulse, 0);
    }

    #[test]
    fn a_slave_that_stops_hearing_the_grandmaster_drops_its_lock_and_pulse() {
        let mut wired = Wired::new(-30_000.0);
        wired.run_ms(5000);
        assert!(wired.slave.node.ptp.is_locked());
        wired.slave.nic.pulse_ready = true;
        let stopped = wired.slave.nic.pulse_stopped;
        for _ in 0..900 {
            wired.gm.nic.t += 1e6;
            wired.slave.nic.t = wired.gm.nic.t;
            wired.slave.tick(1);
        }
        assert!(wired.slave.node.ptp.is_locked());
        for _ in 0..200 {
            wired.gm.nic.t += 1e6;
            wired.slave.nic.t = wired.gm.nic.t;
            wired.slave.tick(1);
        }
        assert!(!wired.slave.node.ptp.is_locked());
        assert_eq!(wired.slave.nic.pulse_stopped, stopped + 1);
        assert!(
            !wired
                .slave
                .unit_info(unit_address(1))
                .flags
                .intersects(Flags::PTP_LOCKED | Flags::SYNC_READY)
        );
        wired.gm.now_ms = wired.slave.now_ms;
        wired.run_ms(3000);
        assert!(wired.slave.node.ptp.is_locked());
        assert_eq!(wired.slave.nic.pulse_armed, 2);
    }

    #[test]
    fn the_drift_is_written_only_when_it_changes() {
        let mut wired = Wired::new(0.0);
        wired.run_ms(5000);
        let exchanges = 5000 / 16;
        assert!(wired.slave.nic.drift_writes < exchanges);
    }

    #[test]
    fn reset_id_forgets_the_role_and_the_lock() {
        let mut wired = Wired::new(0.0);
        wired.run_ms(5000);
        assert!(wired.slave.node.ptp.is_locked());
        wired.slave.send(ALL_NODES, Kind::ResetId, &[]);
        wired.slave.replies();
        wired.slave.tick(50);
        assert!(!wired.slave.node.ptp.is_locked());
        assert_eq!(wired.slave.nic.drift_ppb, 0);
        wired.gm.send(ALL_NODES, Kind::ResetId, &[]);
        wired.gm.replies();
        wired.gm.tick(50);
        assert!(!wired.gm.node.ptp.is_locked());
        assert!(!wired.gm.node.is_grandmaster());
        assert_eq!(
            wired.gm.unit_info(unit_address(UNASSIGNED_ID)).flags,
            Flags::empty()
        );
    }

    #[test]
    fn reset_id_keeps_the_ptp_config() {
        let mut wired = Wired::new(0.0);
        let config = autd3_cpu_wire::config::PtpConfig {
            holdover: core::time::Duration::from_millis(123),
            ..autd3_cpu_wire::config::PtpConfig::default()
        };
        wired.slave.node.configure_ptp(config);
        wired.slave.send(ALL_NODES, Kind::ResetId, &[]);
        wired.slave.replies();
        wired.slave.tick(50);
        assert!(!wired.slave.node.is_assigned());
        assert_eq!(wired.slave.node.ptp.config(), config);
    }
}
