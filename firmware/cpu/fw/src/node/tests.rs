use std::vec;
use std::vec::Vec;

use zerocopy::FromBytes;
use zerocopy::IntoBytes;
use zerocopy::little_endian::U64;

use autd3_cpu_wire::udp::{
    ALL_NODES, AssignIdBody, FLAG_ASSIGNED, FLAG_DOWNSTREAM_LINK, FLAG_DOWNSTREAM_OPEN,
    FLAG_SYNC_READY, FrameReply, HEADER_BYTES, Header, Kind, PORT, PROTOCOL_VERSION, SetTimeBody,
    Status, UNASSIGNED_ID, UnblockReply, UnitInfo, mac, solicited_node, unit_address,
};

use super::{CommandLayer, Node, Received};
use crate::net::{
    self, ETH_HEADER, ETHERTYPE_IPV6, Endpoint, ICMPV6_ECHO_REQUEST, ICMPV6_NEIGHBOR_SOLICITATION,
    IPV6_HEADER, Ipv6, MAX_FRAME, Mac, NEXT_HEADER_ICMPV6, Packet, UDP_PAYLOAD_OFFSET,
};
use crate::nic::RxMeta;
use crate::proto::{Disposition, FRAME_BYTES_MAX, Reply as CmdReply};
use crate::sim_nic::SimNic;

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
            self.reply = CmdReply::new(frame[0], 0, &[0x5A, 0x5B]);
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
        let mut nic = SimNic::new();
        nic.time = 5_000_000_000;
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
                reply: CmdReply::new(0xFF, 0, &[]),
            },
        }
    }

    fn request(&mut self, dst: Ipv6, version: u8, kind: u8, msg_id: u16, body: &[u8]) -> Received {
        let mut buf = vec![0u8; MAX_FRAME];
        let header = Header {
            version,
            kind,
            msg_id: zerocopy::little_endian::U16::new(msg_id),
        };
        buf[UDP_PAYLOAD_OFFSET..UDP_PAYLOAD_OFFSET + HEADER_BYTES]
            .copy_from_slice(header.as_bytes());
        buf[UDP_PAYLOAD_OFFSET + HEADER_BYTES..UDP_PAYLOAD_OFFSET + HEADER_BYTES + body.len()]
            .copy_from_slice(body);
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
        let len = net::udp(&mut buf, &src, &to, HEADER_BYTES + body.len()).unwrap();
        buf[0..6].copy_from_slice(&[0xAA; 6]);
        self.deliver(&buf[..len])
    }

    fn deliver(&mut self, frame: &[u8]) -> Received {
        let rx = RxMeta { port: UPSTREAM };
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
        let body = AssignIdBody { unit_id: id };
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
    assert_eq!(info.flags, 0);
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
    let body = AssignIdBody { unit_id: 3 };
    h.send(ALL_NODES, Kind::AssignId, body.as_bytes());
    assert!(h.replies().is_empty());
    assert!(!h.node.is_assigned());

    h.send(unit_address(UNASSIGNED_ID), Kind::AssignId, body.as_bytes());
    let r = h.only_reply();
    assert_eq!(r.body, [Status::Ok.as_u8()]);
    assert_eq!(r.src_ip, unit_address(3));
    assert_eq!(r.src_mac, mac(3));
    assert_eq!(h.nic.mac, Some(mac(3)));
    assert_eq!(h.node.unit_id(), 3);

    h.send(ALL_NODES, Kind::Discover, &[]);
    assert!(h.replies().is_empty());
    h.send(unit_address(3), Kind::AssignId, body.as_bytes());
    assert!(h.replies().is_empty());
    h.send(unit_address(UNASSIGNED_ID), Kind::ReadUnitInfo, &[]);
    assert!(h.replies().is_empty());

    let info = h.unit_info(unit_address(3));
    assert_eq!(info.unit_id, 3);
    assert_eq!(info.flags, FLAG_ASSIGNED);
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
    };
    h.send(unit_address(UNASSIGNED_ID), Kind::AssignId, body.as_bytes());
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
        FLAG_ASSIGNED | FLAG_DOWNSTREAM_OPEN | FLAG_DOWNSTREAM_LINK
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
fn every_assigned_unit_takes_the_host_time_and_arms_its_pulse() {
    for id in [0, 2] {
        let mut h = Harness::new();
        h.assign(id);
        let t = SetTimeBody {
            sys_time: U64::new(812_000_000_123_456_789),
        };
        h.send(unit_address(id), Kind::SetTime, &t.as_bytes()[..4]);
        assert_eq!(h.only_reply().body, [Status::InvalidPayload.as_u8()]);
        h.send(ALL_NODES, Kind::SetTime, t.as_bytes());
        assert!(h.replies().is_empty());
        assert_eq!(h.nic.time_set, None);

        h.send(unit_address(id), Kind::SetTime, t.as_bytes());
        assert_eq!(h.only_reply().body, [Status::Ok.as_u8()]);
        assert_eq!(h.nic.time_set, Some(812_000_000_123_456_789));
        assert_eq!(h.nic.pulse_armed, 1);
        assert_eq!(h.unit_info(unit_address(id)).flags, FLAG_ASSIGNED);

        h.nic.pulse_ready = true;
        let info = h.unit_info(unit_address(id));
        assert_eq!(info.flags, FLAG_ASSIGNED | FLAG_SYNC_READY);
        assert_eq!(info.sys_time.get(), 812_000_000_123_456_789);
    }
}

#[test]
fn a_frame_reaches_the_command_layer_and_is_answered_at_once() {
    let mut h = Harness::new();
    let body = frame_body(7);
    assert_eq!(
        h.send(unit_address(UNASSIGNED_ID), Kind::Frame, &body),
        Received::Nothing
    );
    assert!(h.cmds.frames.is_empty());
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
    assert_eq!(reply.flags, FLAG_ASSIGNED);
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

    h.node
        .send_completion(&mut h.nic, 0x0101, &CmdReply::new(3, 0x02, &[]));
    let r = h.only_reply();
    assert_eq!(r.header, Header::new(Kind::Frame, 0x0101));
    assert_eq!(r.port, UPSTREAM);
    assert_eq!(r.dst_ip, HOST_IP);
    assert_eq!(r.dst_port, HOST_PORT);
    assert_eq!(r.src_ip, unit_address(2));
    let (reply, data) = parse_frame_reply(&r.body);
    assert_eq!(reply.ack, 3);
    assert_eq!(reply.status, 0x02);
    assert!(data.is_empty());
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
    h.node
        .send_completion(&mut h.nic, 1, &CmdReply::new(0, 0, &[]));
    assert!(h.replies().is_empty());

    h.assign(0);
    h.node
        .send_completion(&mut h.nic, 1, &CmdReply::new(0, 0, &[]));
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

    h.cmds.reply = CmdReply::new(0x33, 0, &[1, 2, 3]);
    assert_eq!(
        h.send(unit_address(4), Kind::Heartbeat, &[]),
        Received::HostMessage
    );
    assert!(h.cmds.frames.is_empty());
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
    assert_eq!(info.flags, 0);
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
    frame[12..14].copy_from_slice(&ETHERTYPE_IPV6.to_be_bytes());
    let ip = &mut frame[ETH_HEADER..];
    ip[0] = 0x60;
    ip[4..6].copy_from_slice(&(icmp.len() as u16).to_be_bytes());
    ip[6] = NEXT_HEADER_ICMPV6;
    ip[7] = hop_limit;
    ip[8..24].copy_from_slice(&HOST_IP);
    ip[24..40].copy_from_slice(&dst);
    ip[IPV6_HEADER..].copy_from_slice(icmp);
    frame
}

fn solicitation(target: Ipv6) -> Vec<u8> {
    let mut icmp = vec![0u8; 32];
    icmp[0] = ICMPV6_NEIGHBOR_SOLICITATION;
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
    let mut icmp = vec![ICMPV6_ECHO_REQUEST, 0, 0, 0, 0, 1, 0, 2, b'x'];
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
