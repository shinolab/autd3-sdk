use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};
use std::time::Duration;

use autd3_cpu_fw::proto::Reply;
use autd3_cpu_wire::payload::Telemetry;
use autd3_cpu_wire::udp::{
    AssignIdBody, DEVICE_QUEUE_FRAMES, Flags, FrameReply, Header, Kind, PROTOCOL_VERSION, Role,
    SetTimeBody, Status, UNASSIGNED_ID, UPSTREAM_UNKNOWN, UnblockReply, UnitInfo,
};
use autd3_rs_core::Interface;
use autd3_rs_core::protocol::Cmd;
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use zerocopy::little_endian::U64;
use zerocopy::{FromBytes, IntoBytes};

const QUIET: Duration = Duration::from_millis(50);
const PATIENCE: Duration = Duration::from_secs(5);
const SYNCED: Flags = Flags::PTP_LOCKED.union(Flags::SYNC_READY);

fn frame(seq: u8, cmd: Cmd, payload: &[u8]) -> Vec<u8> {
    [&[seq, cmd.as_u8()], payload].concat()
}

struct Answer {
    src: SocketAddrV6,
    header: Header,
    body: Vec<u8>,
}

impl Answer {
    fn status(&self) -> u8 {
        self.body[0]
    }

    fn unit_info(&self) -> UnitInfo {
        UnitInfo::read_from_prefix(&self.body[1..]).unwrap().0
    }

    fn frame_reply(&self) -> (FrameReply, &[u8]) {
        FrameReply::read_from_prefix(&self.body).unwrap()
    }
}

struct Host {
    socket: UdpSocket,
    msg_id: u16,
}

impl Host {
    fn new() -> Self {
        let socket = UdpSocket::bind(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0)).unwrap();
        Self { socket, msg_id: 0 }
    }

    fn request(&mut self, dst: SocketAddrV6, kind: Kind, body: &[u8]) -> u16 {
        self.msg_id += 1;
        self.send(dst, Header::new(kind, self.msg_id), body);
        self.msg_id
    }

    fn send(&self, dst: SocketAddrV6, header: Header, body: &[u8]) {
        let datagram = [header.as_bytes(), body].concat();
        self.socket.send_to(&datagram, dst).unwrap();
    }

    fn receive(&self, within: Duration) -> Option<Answer> {
        let mut buf = [0u8; 2048];
        self.socket.set_read_timeout(Some(within)).unwrap();
        let (len, src) = self.socket.recv_from(&mut buf).ok()?;
        let SocketAddr::V6(src) = src else {
            panic!("the emulator answers over IPv6");
        };
        let (header, body) = Header::read_from_prefix(&buf[..len]).unwrap();
        Some(Answer {
            src,
            header,
            body: body.to_vec(),
        })
    }

    fn collect(&self) -> Vec<Answer> {
        std::iter::from_fn(|| self.receive(QUIET)).collect()
    }

    fn ask(&mut self, dst: SocketAddrV6, kind: Kind, body: &[u8]) -> Vec<Answer> {
        let msg_id = self.request(dst, kind, body);
        let answers = self.collect();
        assert!(answers.iter().all(|a| a.header.msg_id.get() == msg_id));
        assert!(answers.iter().all(|a| a.header.kind == kind.as_u8()));
        answers
    }

    fn ask_one(&mut self, dst: SocketAddrV6, kind: Kind, body: &[u8]) -> Answer {
        let mut answers = self.ask(dst, kind, body);
        assert_eq!(answers.len(), 1);
        answers.remove(0)
    }

    fn unit_infos(&mut self, emulator: &UdpEmulator) -> Vec<UnitInfo> {
        let mut infos: Vec<(usize, UnitInfo)> = self
            .ask(emulator.addr(), Kind::ReadUnitInfo, &[])
            .iter()
            .map(|answer| {
                assert_eq!(answer.status(), Status::Ok.as_u8());
                (index_of(emulator, answer.src), answer.unit_info())
            })
            .collect();
        infos.sort_by_key(|&(index, _)| index);
        assert!(infos.iter().map(|&(index, _)| index).eq(0..infos.len()));
        infos.into_iter().map(|(_, info)| info).collect()
    }

    fn assign(&mut self, emulator: &UdpEmulator, index: usize) -> bool {
        let dst = emulator.device_addr(index);
        let body = AssignIdBody {
            unit_id: u8::try_from(index).unwrap(),
            role: if index == 0 {
                Role::Grandmaster
            } else {
                Role::Slave
            }
            .as_u8(),
        };
        let assigned = self.ask_one(dst, Kind::AssignId, body.as_bytes());
        assert_eq!(assigned.status(), Status::Ok.as_u8());
        assert_eq!(assigned.src, emulator.device_addr(index));
        let unblocked = self.ask_one(assigned.src, Kind::UnblockDownstream, &[]);
        assert_eq!(unblocked.status(), Status::Ok.as_u8());
        UnblockReply::read_from_prefix(&unblocked.body[1..])
            .unwrap()
            .0
            .downstream_link
            != 0
    }

    fn enumerate(&mut self, emulator: &UdpEmulator) {
        for index in 0..emulator.num_devices() {
            self.assign(emulator, index);
        }
    }

    fn set_time(&mut self, emulator: &UdpEmulator, sys_time: u64) -> u8 {
        let body = SetTimeBody {
            sys_time: U64::new(sys_time),
        };
        self.ask_one(emulator.device_addr(0), Kind::SetTime, body.as_bytes())
            .status()
    }

    fn frame(&mut self, emulator: &UdpEmulator, index: usize, bytes: &[u8]) -> Vec<Answer> {
        self.ask(emulator.device_addr(index), Kind::Frame, bytes)
    }

    fn accepted(&mut self, emulator: &UdpEmulator, index: usize, bytes: &[u8]) -> Answer {
        let mut answers = self.frame(emulator, index, bytes);
        assert_eq!(answers.len(), 1);
        answers.remove(0)
    }

    fn reset(&mut self, emulator: &UdpEmulator, index: usize) {
        let answer = self.accepted(emulator, index, &frame(0, Cmd::Reset, &[]));
        assert_eq!(answer.frame_reply().0.ack, Reply::RESET.ack);
    }

    fn telemetry(&mut self, emulator: &UdpEmulator, index: usize, seq: u8) -> Telemetry {
        let answer = self.accepted(emulator, index, &frame(seq, Cmd::ReadTelemetry, &[]));
        let (head, data) = answer.frame_reply();
        assert_eq!(head.ack, seq);
        Telemetry::read_from_prefix(data).unwrap().0
    }
}

fn index_of(emulator: &UdpEmulator, src: SocketAddrV6) -> usize {
    (0..emulator.num_devices())
        .find(|&index| emulator.device_addr(index).port() == src.port())
        .unwrap()
}

fn fan(emulator: &UdpEmulator, index: usize) -> bool {
    emulator.with_device(index, |device| device.fpga().force_fan())
}

fn fan_on(seq: u8) -> Vec<u8> {
    frame(seq, Cmd::ForceFan, &[1])
}

fn enumerated(num_devices: usize) -> (UdpEmulator, Host) {
    let emulator = UdpEmulator::spawn(num_devices).unwrap();
    let mut host = Host::new();
    host.enumerate(&emulator);
    (emulator, host)
}

#[test]
fn the_emulator_exposes_its_group_and_device_addresses() {
    let emulator = UdpEmulator::spawn(3).unwrap();

    assert_eq!(emulator.num_devices(), 3);
    assert_eq!(emulator.interface(), Interface::Addr(emulator.addr()));
    assert_eq!(*emulator.addr().ip(), Ipv6Addr::LOCALHOST);
    let mut ports: Vec<u16> = (0..3).map(|i| emulator.device_addr(i).port()).collect();
    ports.push(emulator.addr().port());
    ports.sort_unstable();
    ports.dedup();
    assert_eq!(ports.len(), 4);
    assert_eq!(emulator.with_devices(|devices| devices.len()), 3);
}

#[test]
fn only_the_head_of_a_fresh_chain_is_reachable() {
    let emulator = UdpEmulator::spawn(3).unwrap();
    let mut host = Host::new();

    let discovered = host.ask_one(emulator.addr(), Kind::Discover, &[]);
    let infos = host.unit_infos(&emulator);

    assert_eq!(discovered.src, emulator.device_addr(0));
    assert_eq!(discovered.status(), Status::Ok.as_u8());
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].unit_id, UNASSIGNED_ID);
    assert_eq!(infos[0].upstream_port, UPSTREAM_UNKNOWN);
    assert_eq!(infos[0].flags, Flags::DOWNSTREAM_LINK);
    assert!(
        host.ask(emulator.device_addr(1), Kind::ReadUnitInfo, &[])
            .is_empty()
    );
}

#[test]
fn assigning_and_unblocking_walks_down_the_chain() {
    let emulator = UdpEmulator::spawn(3).unwrap();
    let mut host = Host::new();

    for index in 0..3 {
        let discovered = host.ask_one(emulator.addr(), Kind::Discover, &[]);
        assert_eq!(discovered.src, emulator.device_addr(index));
        let downstream_link = host.assign(&emulator, index);
        assert_eq!(downstream_link, index < 2);
    }

    assert!(host.ask(emulator.addr(), Kind::Discover, &[]).is_empty());
    let infos = host.unit_infos(&emulator);
    assert_eq!(infos.len(), 3);
    for (index, info) in infos.iter().enumerate() {
        assert_eq!(usize::from(info.unit_id), index);
        assert_eq!(info.upstream_port, 0);
        assert!(
            info.flags
                .contains(Flags::ASSIGNED | Flags::DOWNSTREAM_OPEN)
        );
        assert_eq!(info.flags.contains(Flags::GRANDMASTER), index == 0);
        assert_eq!(info.flags.contains(Flags::DOWNSTREAM_LINK), index < 2);
        assert_eq!(
            info.fw_version,
            [
                autd3_cpu_fw::FW_VERSION_MAJOR,
                autd3_cpu_fw::FW_VERSION_MINOR,
                autd3_cpu_fw::FW_VERSION_PATCH
            ]
        );
    }
}

#[test]
fn an_assignment_is_refused_when_it_is_malformed_and_ignored_when_repeated() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let mut host = Host::new();
    let dst = emulator.device_addr(0);
    let invalid = Status::InvalidPayload.as_u8();

    assert_eq!(host.ask_one(dst, Kind::AssignId, &[0]).status(), invalid);
    assert_eq!(host.ask_one(dst, Kind::AssignId, &[0, 9]).status(), invalid);
    let unassigned = [UNASSIGNED_ID, Role::Slave.as_u8()];
    assert_eq!(
        host.ask_one(dst, Kind::AssignId, &unassigned).status(),
        invalid
    );
    assert_eq!(
        host.ask_one(dst, Kind::UnblockDownstream, &[]).status(),
        Status::NotAssigned.as_u8()
    );

    host.assign(&emulator, 0);

    let again = [5, Role::Slave.as_u8()];
    let assigned = emulator.device_addr(0);
    assert!(host.ask(assigned, Kind::AssignId, &again).is_empty());
    assert_eq!(host.unit_infos(&emulator)[0].unit_id, 0);
}

#[test]
fn unicast_only_requests_are_ignored_on_the_group_address() {
    let (emulator, mut host) = enumerated(2);
    let group = emulator.addr();

    for kind in [
        Kind::Frame,
        Kind::Heartbeat,
        Kind::AssignId,
        Kind::UnblockDownstream,
        Kind::SetTime,
    ] {
        assert!(host.ask(group, kind, &fan_on(0)).is_empty());
    }

    assert!(!fan(&emulator, 0));
    assert_eq!(emulator.heartbeats_received(0), 0);
}

#[test]
fn a_request_of_another_protocol_version_is_answered_with_the_supported_one() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let host = Host::new();
    let header = Header {
        version: PROTOCOL_VERSION + 1,
        ..Header::new(Kind::ReadUnitInfo, 7)
    };

    host.send(emulator.addr(), header, &[]);
    let answer = host.receive(PATIENCE).unwrap();

    assert_eq!(answer.header.version, PROTOCOL_VERSION);
    assert_eq!(answer.header.msg_id.get(), 7);
    assert_eq!(answer.body, [Status::UnsupportedVersion.as_u8()]);
}

#[test]
fn malformed_datagrams_are_ignored() {
    let (emulator, mut host) = enumerated(1);
    let dst = emulator.device_addr(0);

    host.socket.send_to(&[PROTOCOL_VERSION], dst).unwrap();
    let unknown_kind = Header {
        kind: 0x7F,
        ..Header::new(Kind::Frame, 1)
    };
    host.send(dst, unknown_kind, &[]);
    assert!(host.collect().is_empty());
    assert!(host.frame(&emulator, 0, &[0]).is_empty());

    host.reset(&emulator, 0);
}

#[test]
fn setting_the_time_is_only_accepted_by_the_grandmaster() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut host = Host::new();
    let time = SetTimeBody {
        sys_time: U64::new(1_000_000_000),
    };

    assert_eq!(
        host.set_time(&emulator, 1_000_000_000),
        Status::NotAssigned.as_u8()
    );
    host.enumerate(&emulator);
    assert_eq!(
        host.ask_one(emulator.device_addr(1), Kind::SetTime, time.as_bytes())
            .status(),
        Status::NotGrandmaster.as_u8()
    );
    assert_eq!(
        host.ask_one(emulator.device_addr(0), Kind::SetTime, &[0; 3])
            .status(),
        Status::InvalidPayload.as_u8()
    );
    assert!(
        host.unit_infos(&emulator)
            .iter()
            .all(|info| !info.flags.intersects(SYNCED))
    );

    assert_eq!(
        host.set_time(&emulator, (u64::from(u32::MAX) + 1) * 1_000_000_000),
        Status::InvalidPayload.as_u8()
    );
    assert!(
        host.unit_infos(&emulator)
            .iter()
            .all(|info| !info.flags.intersects(SYNCED))
    );

    assert_eq!(host.set_time(&emulator, 1_000_000_000), Status::Ok.as_u8());
    assert_eq!(
        host.set_time(&emulator, u64::from(u32::MAX) * 1_000_000_000),
        Status::Ok.as_u8()
    );
}

#[test]
fn a_frame_runs_on_the_addressed_unit_and_is_acknowledged_with_its_sequence() {
    let (emulator, mut host) = enumerated(2);
    host.reset(&emulator, 1);

    let answer = host.accepted(&emulator, 1, &fan_on(0));

    let (head, data) = answer.frame_reply();
    assert_eq!(answer.src, emulator.device_addr(1));
    assert_eq!(head.ack, 0);
    assert_eq!(head.status, 0);
    assert_eq!(head.unit_id, 1);
    assert!(head.flags.contains(Flags::ASSIGNED));
    assert_eq!(data, [0u8; 0]);
    assert!(fan(&emulator, 1));
    assert!(!fan(&emulator, 0));
}

#[test]
fn a_frame_to_an_unassigned_unit_is_ignored() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let mut host = Host::new();

    assert!(
        host.frame(&emulator, 0, &frame(0, Cmd::Reset, &[]))
            .is_empty()
    );
    assert!(
        host.ask(emulator.device_addr(0), Kind::Heartbeat, &[])
            .is_empty()
    );
    assert_eq!(emulator.heartbeats_received(0), 0);
}

#[test]
fn a_heartbeat_is_answered_with_the_last_acknowledgement() {
    let (emulator, mut host) = enumerated(1);
    host.reset(&emulator, 0);
    host.accepted(&emulator, 0, &fan_on(0));
    host.accepted(&emulator, 0, &frame(1, Cmd::Nop, &[]));

    let answer = host.ask_one(emulator.device_addr(0), Kind::Heartbeat, &[]);

    let (head, _) = answer.frame_reply();
    assert_eq!(head.ack, 1);
    assert_eq!(head.unit_id, 0);
    assert_eq!(emulator.heartbeats_received(0), 1);
}

#[test]
fn a_reboot_forgets_the_id_and_the_state_and_cuts_the_units_behind() {
    let (emulator, mut host) = enumerated(3);
    for index in 0..3 {
        host.reset(&emulator, index);
        host.accepted(&emulator, index, &fan_on(0));
    }

    emulator.reboot(1);

    assert!((0..3).map(|i| fan(&emulator, i)).eq([true, false, true]));
    let infos = host.unit_infos(&emulator);
    assert_eq!(infos.len(), 2);
    assert_eq!(infos[0].unit_id, 0);
    assert_eq!(infos[1].unit_id, UNASSIGNED_ID);
    assert!(host.frame(&emulator, 1, &fan_on(1)).is_empty());
    assert!(host.frame(&emulator, 2, &fan_on(1)).is_empty());
    host.accepted(&emulator, 0, &frame(1, Cmd::Nop, &[]));

    let discovered = host.ask_one(emulator.addr(), Kind::Discover, &[]);
    assert_eq!(discovered.src, emulator.device_addr(1));
}

#[test]
fn a_dropped_frame_never_reaches_the_firmware() {
    let (emulator, mut host) = enumerated(2);
    host.reset(&emulator, 0);
    host.reset(&emulator, 1);

    emulator.drop_next_frames(0, 2);

    assert!(host.frame(&emulator, 0, &fan_on(0)).is_empty());
    host.accepted(&emulator, 1, &fan_on(0));
    assert!(host.frame(&emulator, 0, &fan_on(0)).is_empty());
    assert!(!fan(&emulator, 0));
    assert!(fan(&emulator, 1));
    let answer = host.accepted(&emulator, 0, &fan_on(0));
    assert_eq!(answer.frame_reply().0.ack, 0);
    assert!(fan(&emulator, 0));
}

#[test]
fn a_dropped_reply_leaves_the_frame_executed() {
    let (emulator, mut host) = enumerated(1);
    host.reset(&emulator, 0);

    emulator.drop_next_replies(0, 2);

    assert!(host.frame(&emulator, 0, &fan_on(0)).is_empty());
    assert!(fan(&emulator, 0));
    assert!(
        host.ask(emulator.device_addr(0), Kind::Heartbeat, &[])
            .is_empty()
    );
    assert_eq!(emulator.heartbeats_received(0), 1);
    let answer = host.accepted(&emulator, 0, &frame(1, Cmd::Nop, &[]));
    assert_eq!(answer.frame_reply().0.ack, 1);
}

#[test]
fn a_dropped_unit_info_is_answered_on_the_next_request() {
    let (emulator, mut host) = enumerated(2);

    emulator.drop_next_unit_infos(1, 1);

    let first = host.unit_infos(&emulator);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].unit_id, 0);
    assert_eq!(host.unit_infos(&emulator).len(), 2);
}

#[test]
fn a_duplicated_frame_is_executed_and_answered_once() {
    let (emulator, mut host) = enumerated(1);
    host.reset(&emulator, 0);
    let before = host.telemetry(&emulator, 0, 0);

    emulator.duplicate_next_frames(0, 1);
    let answer = host.accepted(&emulator, 0, &fan_on(1));

    assert_eq!(answer.frame_reply().0.ack, 1);
    let after = host.telemetry(&emulator, 0, 2);
    assert_eq!(after.dedup.get() - before.dedup.get(), 1);
    assert_eq!(after.seq_mismatch.get(), 0);
    assert_eq!(after.processed.get() - before.processed.get(), 2);
    host.accepted(&emulator, 0, &frame(3, Cmd::Nop, &[]));
}

#[test]
fn a_muted_unit_hears_the_host_but_neither_executes_nor_answers() {
    let (emulator, mut host) = enumerated(1);
    host.reset(&emulator, 0);

    emulator.set_muted(0, true);

    assert!(host.frame(&emulator, 0, &fan_on(0)).is_empty());
    assert!(
        host.ask(emulator.device_addr(0), Kind::Heartbeat, &[])
            .is_empty()
    );
    assert!(!fan(&emulator, 0));
    assert_eq!(emulator.heartbeats_received(0), 1);
    assert_eq!(host.unit_infos(&emulator).len(), 1);

    emulator.set_muted(0, false);
    host.accepted(&emulator, 0, &fan_on(0));
    assert!(fan(&emulator, 0));
}

#[test]
fn a_frame_out_of_sequence_is_answered_with_the_last_accepted_one() {
    let (emulator, mut host) = enumerated(1);
    host.reset(&emulator, 0);
    host.accepted(&emulator, 0, &frame(0, Cmd::Nop, &[]));

    let answer = host.accepted(&emulator, 0, &fan_on(5));

    assert_eq!(answer.frame_reply().0.ack, 0);
    assert!(!fan(&emulator, 0));
    assert_eq!(host.telemetry(&emulator, 0, 1).seq_mismatch.get(), 1);
}

fn silent_at(host: &mut Host, addr: SocketAddrV6) -> bool {
    host.ask(addr, Kind::ReadUnitInfo, &[]).is_empty()
        && host.ask(addr, Kind::Heartbeat, &[]).is_empty()
        && host
            .ask(addr, Kind::Frame, &frame(0, Cmd::Reset, &[]))
            .is_empty()
}

#[test]
fn an_assignment_moves_the_unit_to_a_new_address() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let mut host = Host::new();
    let unassigned = emulator.device_addr(0);

    host.assign(&emulator, 0);

    let assigned = emulator.device_addr(0);
    assert_ne!(assigned.port(), unassigned.port());
    assert!(silent_at(&mut host, unassigned));
    assert_eq!(emulator.heartbeats_received(0), 0);
    let info = host.ask_one(assigned, Kind::ReadUnitInfo, &[]);
    assert_eq!(info.src, assigned);
    assert_eq!(info.unit_info().unit_id, 0);
}

#[test]
fn a_reboot_abandons_the_address_of_the_unit() {
    let (emulator, mut host) = enumerated(2);
    let assigned = emulator.device_addr(0);
    let behind = emulator.device_addr(1);
    host.reset(&emulator, 0);

    emulator.reboot(0);

    assert_ne!(emulator.device_addr(0).port(), assigned.port());
    assert_eq!(emulator.device_addr(1), behind);
    assert!(silent_at(&mut host, assigned));
    assert!(!fan(&emulator, 0));
    let rebooted = host.ask_one(emulator.device_addr(0), Kind::ReadUnitInfo, &[]);
    assert_eq!(rebooted.unit_info().unit_id, UNASSIGNED_ID);
}

#[test]
fn a_paused_main_loop_queues_frames_and_drops_the_one_past_the_queue() {
    let (emulator, mut host) = enumerated(1);
    let unit = emulator.device_addr(0);
    host.reset(&emulator, 0);
    let queue = u8::try_from(DEVICE_QUEUE_FRAMES).unwrap();

    emulator.pause_main_loop(0);
    let queued: Vec<u16> = (0..queue)
        .map(|seq| host.request(unit, Kind::Frame, &frame(seq, Cmd::Nop, &[])))
        .collect();
    host.request(unit, Kind::Frame, &fan_on(queue));
    assert!(host.collect().is_empty());

    let heartbeat = host.ask_one(unit, Kind::Heartbeat, &[]);
    assert_eq!(heartbeat.frame_reply().0.ack, Reply::RESET.ack);
    assert_eq!(emulator.heartbeats_received(0), 1);

    assert!(emulator.step_main_loop(0));
    let first = host.receive(PATIENCE).unwrap();
    assert_eq!(first.header.kind, Kind::Frame.as_u8());
    assert_eq!(first.header.msg_id.get(), queued[0]);
    assert_eq!(first.frame_reply().0.ack, 0);
    assert!(host.collect().is_empty());

    emulator.resume_main_loop(0);
    let rest = host.collect();
    assert!(
        rest.iter()
            .map(|answer| answer.header.msg_id.get())
            .eq(queued[1..].iter().copied())
    );
    assert!(
        rest.iter()
            .map(|answer| answer.frame_reply().0.ack)
            .eq(1..queue)
    );
    assert!(!emulator.step_main_loop(0));
    assert!(!fan(&emulator, 0));

    let telemetry = host.telemetry(&emulator, 0, queue);
    assert_eq!(telemetry.fifo_drop.get(), 1);
    assert_eq!(telemetry.seq_mismatch.get(), 0);
    host.accepted(&emulator, 0, &fan_on(queue + 1));
    assert!(fan(&emulator, 0));
}

#[test]
fn a_frame_delivered_twice_to_a_paused_main_loop_is_answered_once_it_has_run() {
    let (emulator, mut host) = enumerated(1);
    let unit = emulator.device_addr(0);
    host.reset(&emulator, 0);

    emulator.pause_main_loop(0);
    assert!(host.ask(unit, Kind::Frame, &fan_on(0)).is_empty());
    assert!(host.ask(unit, Kind::Frame, &fan_on(0)).is_empty());
    assert!(!fan(&emulator, 0));

    emulator.drop_next_replies(0, 1);
    assert!(emulator.step_main_loop(0));
    assert!(fan(&emulator, 0));
    assert!(host.collect().is_empty());

    let repeated = host.ask_one(unit, Kind::Frame, &fan_on(0));
    assert_eq!(repeated.frame_reply().0.ack, 0);
    emulator.resume_main_loop(0);
    let telemetry = host.telemetry(&emulator, 0, 1);
    assert_eq!(telemetry.dedup.get(), 2);
    assert_eq!(telemetry.fifo_drop.get(), 0);
}
