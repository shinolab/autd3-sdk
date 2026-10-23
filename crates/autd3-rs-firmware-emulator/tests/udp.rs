use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};
use std::time::Duration;

use autd3_cpu_wire::payload::Telemetry;
use autd3_cpu_wire::udp::{
    AssignIdBody, DEVICE_QUEUE_FRAMES, Flags, FrameReply, Header, Kind, PROTOCOL_VERSION, Role,
    SetTimeBody, Status, UNASSIGNED_ID, UnblockReply, UnitInfo,
};
use autd3_rs_core::Interface;
use autd3_rs_core::protocol::Cmd;
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use zerocopy::little_endian::U64;
use zerocopy::{FromBytes, IntoBytes};

const QUIET: Duration = Duration::from_millis(50);
const PATIENCE: Duration = Duration::from_secs(5);
const SYNCED: Flags = Flags::PTP_LOCKED.union(Flags::SYNC_READY);

fn frame(cmd: Cmd, payload: &[u8]) -> Vec<u8> {
    [&[0, cmd.as_u8(), 0, 0, 0, 0], payload].concat()
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
        let msg_id = self.request(dst, kind, body);
        let answer = self.receive(PATIENCE).unwrap();
        assert_eq!(answer.header.msg_id.get(), msg_id);
        assert_eq!(answer.header.kind, kind.as_u8());
        assert!(self.collect().is_empty());
        answer
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
        self.ask_one(emulator.device_addr(index), Kind::Frame, bytes)
    }

    fn telemetry(&mut self, emulator: &UdpEmulator, index: usize) -> Telemetry {
        let answer = self.accepted(emulator, index, &frame(Cmd::ReadTelemetry, &[]));
        let (_, data) = answer.frame_reply();
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

fn fan_on() -> Vec<u8> {
    frame(Cmd::ForceFan, &[1])
}

fn enumerated(num_devices: usize) -> (UdpEmulator, Host) {
    let emulator = UdpEmulator::spawn(num_devices).unwrap();
    let mut host = Host::new();
    host.enumerate(&emulator);
    (emulator, host)
}

#[test]
#[ignore = "depends on real time"]
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
#[ignore = "depends on real time"]
fn only_the_head_of_a_fresh_chain_is_reachable() {
    let emulator = UdpEmulator::spawn(3).unwrap();
    let mut host = Host::new();

    let discovered = host.ask_one(emulator.addr(), Kind::Discover, &[]);
    let infos = host.unit_infos(&emulator);

    assert_eq!(discovered.src, emulator.device_addr(0));
    assert_eq!(discovered.status(), Status::Ok.as_u8());
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].unit_id, UNASSIGNED_ID);
    assert_eq!(infos[0].flags, Flags::DOWNSTREAM_LINK);
    assert!(
        host.ask(emulator.device_addr(1), Kind::ReadUnitInfo, &[])
            .is_empty()
    );
}

#[test]
#[ignore = "depends on real time"]
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
        assert!(
            info.flags
                .contains(Flags::ASSIGNED | Flags::DOWNSTREAM_OPEN)
        );
        assert_eq!(info.flags.contains(Flags::GRANDMASTER), index == 0);
        assert_eq!(info.flags.contains(Flags::DOWNSTREAM_LINK), index < 2);
    }
}

#[test]
#[ignore = "depends on real time"]
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
#[ignore = "depends on real time"]
fn unicast_only_requests_are_ignored_on_the_group_address() {
    let (emulator, mut host) = enumerated(2);
    let group = emulator.addr();

    for kind in [
        Kind::Frame,
        Kind::AssignId,
        Kind::UnblockDownstream,
        Kind::SetTime,
    ] {
        assert!(host.ask(group, kind, &fan_on()).is_empty());
    }

    assert!(!fan(&emulator, 0));
}

#[test]
#[ignore = "depends on real time"]
fn a_heartbeat_on_the_group_address_is_answered_by_every_assigned_unit() {
    let (emulator, mut host) = enumerated(2);

    let answers = host.ask(emulator.addr(), Kind::Heartbeat, &[]);

    let mut units: Vec<u8> = answers
        .iter()
        .map(|answer| answer.frame_reply().0.unit_id)
        .collect();
    units.sort_unstable();
    assert_eq!(units, [0, 1]);
    assert_eq!(emulator.heartbeats_received(0), 1);
    assert_eq!(emulator.heartbeats_received(1), 1);

    emulator.reboot(1);

    let answers = host.ask(emulator.addr(), Kind::Heartbeat, &[]);

    assert_eq!(answers.len(), 1);
    assert_eq!(answers[0].frame_reply().0.unit_id, 0);
}

#[test]
#[ignore = "depends on real time"]
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
#[ignore = "depends on real time"]
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
}

#[test]
#[ignore = "depends on real time"]
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
#[ignore = "depends on real time"]
fn a_frame_runs_on_the_addressed_unit_and_is_answered() {
    let (emulator, mut host) = enumerated(2);

    let answer = host.accepted(&emulator, 1, &fan_on());

    let (head, data) = answer.frame_reply();
    assert_eq!(answer.src, emulator.device_addr(1));
    assert_eq!(head.reserved, 0);
    assert_eq!(head.status, 0);
    assert_eq!(head.unit_id, 1);
    assert!(head.flags.contains(Flags::ASSIGNED));
    assert_eq!(data, [0u8; 0]);
    assert!(fan(&emulator, 1));
    assert!(!fan(&emulator, 0));
}

#[test]
#[ignore = "depends on real time"]
fn a_frame_to_an_unassigned_unit_is_ignored() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let mut host = Host::new();

    assert!(host.frame(&emulator, 0, &frame(Cmd::Nop, &[])).is_empty());
    assert!(
        host.ask(emulator.device_addr(0), Kind::Heartbeat, &[])
            .is_empty()
    );
    assert_eq!(emulator.heartbeats_received(0), 0);
    assert_eq!(emulator.frames_received(0), 0);
}

#[test]
#[ignore = "depends on real time"]
fn a_heartbeat_is_answered_with_the_last_result() {
    let (emulator, mut host) = enumerated(1);
    host.accepted(&emulator, 0, &fan_on());
    let read = host.accepted(&emulator, 0, &frame(Cmd::ReadFpgaState, &[]));

    let answer = host.ask_one(emulator.device_addr(0), Kind::Heartbeat, &[]);

    let (head, data) = answer.frame_reply();
    assert_eq!(data, read.frame_reply().1);
    assert_eq!(head.unit_id, 0);
    assert_eq!(emulator.heartbeats_received(0), 1);
}

#[test]
#[ignore = "depends on real time"]
fn a_reboot_forgets_the_id_and_the_state_and_cuts_the_units_behind() {
    let (emulator, mut host) = enumerated(3);
    for index in 0..3 {
        host.accepted(&emulator, index, &fan_on());
    }

    emulator.reboot(1);

    assert!((0..3).map(|i| fan(&emulator, i)).eq([true, false, true]));
    let infos = host.unit_infos(&emulator);
    assert_eq!(infos.len(), 2);
    assert_eq!(infos[0].unit_id, 0);
    assert_eq!(infos[1].unit_id, UNASSIGNED_ID);
    assert!(host.frame(&emulator, 1, &fan_on()).is_empty());
    assert!(host.frame(&emulator, 2, &fan_on()).is_empty());
    host.accepted(&emulator, 0, &frame(Cmd::Nop, &[]));

    let discovered = host.ask_one(emulator.addr(), Kind::Discover, &[]);
    assert_eq!(discovered.src, emulator.device_addr(1));
}

#[test]
#[ignore = "depends on real time"]
fn a_dropped_frame_never_reaches_the_firmware() {
    let (emulator, mut host) = enumerated(2);

    emulator.drop_next_frames(0, 2);

    assert!(host.frame(&emulator, 0, &fan_on()).is_empty());
    host.accepted(&emulator, 1, &fan_on());
    assert!(host.frame(&emulator, 0, &fan_on()).is_empty());
    assert!(!fan(&emulator, 0));
    assert!(fan(&emulator, 1));
    let answer = host.accepted(&emulator, 0, &fan_on());
    assert_eq!(answer.frame_reply().0.status, 0);
    assert!(fan(&emulator, 0));
    assert_eq!(emulator.frames_received(0), 3);
    assert_eq!(emulator.frames_received(1), 1);
}

#[test]
#[ignore = "depends on real time"]
fn a_dropped_reply_leaves_the_frame_executed() {
    let (emulator, mut host) = enumerated(1);

    emulator.drop_next_replies(0, 2);

    assert!(host.frame(&emulator, 0, &fan_on()).is_empty());
    assert!(fan(&emulator, 0));
    assert!(
        host.ask(emulator.device_addr(0), Kind::Heartbeat, &[])
            .is_empty()
    );
    assert_eq!(emulator.heartbeats_received(0), 1);
    let answer = host.accepted(&emulator, 0, &frame(Cmd::Nop, &[]));
    assert_eq!(answer.frame_reply().0.status, 0);
}

#[test]
#[ignore = "depends on real time"]
fn a_dropped_unit_info_is_answered_on_the_next_request() {
    let (emulator, mut host) = enumerated(2);

    emulator.drop_next_unit_infos(1, 1);

    let first = host.unit_infos(&emulator);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].unit_id, 0);
    assert_eq!(host.unit_infos(&emulator).len(), 2);
}

#[test]
#[ignore = "depends on real time"]
fn a_duplicated_frame_is_executed_and_answered_twice() {
    let (emulator, mut host) = enumerated(1);
    let before = host.telemetry(&emulator, 0);

    emulator.duplicate_next_frames(0, 1);
    let answers = host.frame(&emulator, 0, &fan_on());

    assert_eq!(answers.len(), 2);
    assert!(
        answers
            .iter()
            .all(|answer| answer.frame_reply().0.status == 0)
    );
    let after = host.telemetry(&emulator, 0);
    assert_eq!(after.processed.get() - before.processed.get(), 3);
    host.accepted(&emulator, 0, &frame(Cmd::Nop, &[]));
}

#[test]
#[ignore = "depends on real time"]
fn a_muted_unit_hears_the_host_but_neither_executes_nor_answers() {
    let (emulator, mut host) = enumerated(1);

    emulator.set_muted(0, true);

    assert!(host.frame(&emulator, 0, &fan_on()).is_empty());
    assert!(
        host.ask(emulator.device_addr(0), Kind::Heartbeat, &[])
            .is_empty()
    );
    assert!(!fan(&emulator, 0));
    assert_eq!(emulator.heartbeats_received(0), 1);
    assert_eq!(host.unit_infos(&emulator).len(), 1);

    emulator.set_muted(0, false);
    host.accepted(&emulator, 0, &fan_on());
    assert!(fan(&emulator, 0));
}

#[test]
#[ignore = "depends on real time"]
fn a_unit_keeps_its_address_across_assignment_reset_and_reboot() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut host = Host::new();
    let addrs: Vec<SocketAddrV6> = (0..2).map(|i| emulator.device_addr(i)).collect();

    host.enumerate(&emulator);
    assert!(
        (0..2)
            .map(|i| emulator.device_addr(i))
            .eq(addrs.iter().copied())
    );
    let info = host.ask_one(addrs[0], Kind::ReadUnitInfo, &[]);
    assert_eq!(info.src, addrs[0]);
    assert_eq!(info.unit_info().unit_id, 0);

    emulator.reboot(0);
    assert_eq!(emulator.device_addr(0), addrs[0]);
    let rebooted = host.ask_one(addrs[0], Kind::ReadUnitInfo, &[]);
    assert_eq!(rebooted.unit_info().unit_id, UNASSIGNED_ID);
    assert!(host.ask(addrs[0], Kind::Heartbeat, &[]).is_empty());

    host.assign(&emulator, 0);
    assert_eq!(host.ask(addrs[0], Kind::ResetId, &[]).len(), 1);
    let reset = host.ask_one(addrs[0], Kind::ReadUnitInfo, &[]);
    assert_eq!(reset.src, addrs[0]);
    assert_eq!(reset.unit_info().unit_id, UNASSIGNED_ID);
}

#[test]
#[ignore = "depends on real time"]
fn a_reset_id_returns_every_reachable_unit_to_unassigned_at_once() {
    let (emulator, mut host) = enumerated(3);

    let answers = host.ask(emulator.addr(), Kind::ResetId, &[]);

    assert_eq!(answers.len(), 3);
    assert!(answers.iter().all(|a| a.status() == Status::Ok.as_u8()));
    let infos = host.unit_infos(&emulator);
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].unit_id, UNASSIGNED_ID);
    let discovered = host.ask_one(emulator.addr(), Kind::Discover, &[]);
    assert_eq!(discovered.src, emulator.device_addr(0));
}

#[test]
#[ignore = "depends on real time"]
fn setting_the_time_locks_every_assigned_unit_at_once_unless_blocked() {
    let (emulator, mut host) = enumerated(3);
    emulator.set_ptp_lock_blocked(2, true);

    assert_eq!(host.set_time(&emulator, 1_000_000_000), Status::Ok.as_u8());

    let synced: Vec<bool> = host
        .unit_infos(&emulator)
        .iter()
        .map(|info| info.flags.contains(SYNCED))
        .collect();
    assert_eq!(synced, [true, true, false]);

    emulator.set_ptp_lock_blocked(2, false);
    assert!(
        host.unit_infos(&emulator)
            .iter()
            .all(|info| info.flags.contains(SYNCED))
    );

    emulator.set_ptp_lock_blocked(0, true);
    assert!(host.unit_infos(&emulator)[0].flags.contains(SYNCED));
}

#[test]
#[ignore = "depends on real time"]
fn a_unit_assigned_after_the_time_is_set_locks_on_assignment() {
    let emulator = UdpEmulator::spawn(3).unwrap();
    let mut host = Host::new();
    host.assign(&emulator, 0);
    host.assign(&emulator, 1);

    assert_eq!(host.set_time(&emulator, 1_000_000_000), Status::Ok.as_u8());
    host.assign(&emulator, 2);

    let infos = host.unit_infos(&emulator);
    assert_eq!(infos.len(), 3);
    assert!(infos.iter().all(|info| info.flags.contains(SYNCED)));
}

#[test]
#[ignore = "depends on real time"]
fn a_paused_main_loop_queues_frames_and_drops_the_one_past_the_queue() {
    let (emulator, mut host) = enumerated(1);
    let unit = emulator.device_addr(0);

    emulator.pause_main_loop(0);
    let queued: Vec<u16> = (0..DEVICE_QUEUE_FRAMES)
        .map(|_| host.request(unit, Kind::Frame, &frame(Cmd::Nop, &[])))
        .collect();
    host.request(unit, Kind::Frame, &fan_on());
    assert!(host.collect().is_empty());

    let heartbeat = host.ask_one(unit, Kind::Heartbeat, &[]);
    assert_eq!(heartbeat.frame_reply().0.status, 0);
    assert_eq!(emulator.heartbeats_received(0), 1);

    emulator.resume_main_loop(0);
    let answers = host.collect();
    assert!(
        answers
            .iter()
            .all(|answer| answer.header.kind == Kind::Frame.as_u8())
    );
    assert!(
        answers
            .iter()
            .map(|answer| answer.header.msg_id.get())
            .eq(queued.iter().copied())
    );
    assert!(!fan(&emulator, 0));

    let telemetry = host.telemetry(&emulator, 0);
    assert_eq!(telemetry.fifo_drop.get(), 1);
    host.accepted(&emulator, 0, &fan_on());
    assert!(fan(&emulator, 0));
}

#[test]
#[ignore = "depends on real time"]
fn a_frame_delivered_twice_to_a_paused_main_loop_runs_twice() {
    let (emulator, mut host) = enumerated(1);
    let unit = emulator.device_addr(0);
    let before = host.telemetry(&emulator, 0);

    emulator.pause_main_loop(0);
    assert!(host.ask(unit, Kind::Frame, &fan_on()).is_empty());
    assert!(host.ask(unit, Kind::Frame, &fan_on()).is_empty());
    assert!(!fan(&emulator, 0));

    emulator.resume_main_loop(0);
    assert_eq!(host.collect().len(), 2);
    assert!(fan(&emulator, 0));
    let telemetry = host.telemetry(&emulator, 0);
    assert_eq!(telemetry.processed.get() - before.processed.get(), 3);
    assert_eq!(telemetry.fifo_drop.get(), 0);
}
