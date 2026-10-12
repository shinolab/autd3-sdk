use std::net::{Ipv6Addr, SocketAddrV6, UdpSocket};
use std::time::Duration;

use autd3_cpu_wire::udp::{Header, Kind, PROTOCOL_VERSION, Status};
use autd3_rs_core::Interface;
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use zerocopy::{FromBytes, IntoBytes};

const PATIENCE: Duration = Duration::from_secs(5);

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
fn a_request_of_another_protocol_version_is_answered_with_the_supported_one() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let socket = UdpSocket::bind(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0)).unwrap();
    socket.set_read_timeout(Some(PATIENCE)).unwrap();
    let header = Header {
        version: PROTOCOL_VERSION + 1,
        ..Header::new(Kind::ReadUnitInfo, 7)
    };

    socket.send_to(header.as_bytes(), emulator.addr()).unwrap();
    let mut buf = [0u8; 2048];
    let (len, _) = socket.recv_from(&mut buf).unwrap();
    let (answer, body) = Header::read_from_prefix(&buf[..len]).unwrap();

    assert_eq!(answer.version, PROTOCOL_VERSION);
    assert_eq!(answer.msg_id.get(), 7);
    assert_eq!(body, [Status::UnsupportedVersion.as_u8()]);
}
