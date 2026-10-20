mod common;

use std::net::{Ipv6Addr, SocketAddrV6, UdpSocket};

use autd3_cpu_wire::Cmd;
use autd3_cpu_wire::udp::{Header, Kind};
use autd3_rs::commands::{ForceFan, Nop};
use autd3_rs::{Client, Error, Frames, MAX_INFLIGHT, Telemetry};
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use zerocopy::IntoBytes;

use common::{PATIENCE, emulator, open, open_with, option, patient_config};

const FOREIGN_SEQ: u8 = 0x80;

fn fans(emulator: &UdpEmulator) -> Vec<bool> {
    emulator.with_devices(|devices| devices.iter().map(|d| d.fpga().force_fan()).collect())
}

async fn telemetry(client: &Client) -> Vec<Telemetry> {
    client.read_telemetry().await.unwrap()
}

struct Stranger {
    socket: UdpSocket,
    unit: SocketAddrV6,
}

impl Stranger {
    fn of(emulator: &UdpEmulator, device: usize) -> Self {
        let socket = UdpSocket::bind((Ipv6Addr::LOCALHOST, 0)).unwrap();
        socket.set_read_timeout(Some(PATIENCE)).unwrap();
        Self {
            socket,
            unit: emulator.device_addr(device),
        }
    }

    fn occupy_a_queue_slot(&self) {
        let header = Header::new(Kind::Frame, 0);
        let datagram = [
            header.as_bytes(),
            &[FOREIGN_SEQ, Cmd::Nop.as_u8(), 0, 0, 0, 0],
        ]
        .concat();
        self.socket.send_to(&datagram, self.unit).unwrap();
        self.wait_until_everything_sent_was_received();
    }

    fn wait_until_everything_sent_was_received(&self) {
        let header = Header::new(Kind::ReadUnitInfo, 1);
        self.socket.send_to(header.as_bytes(), self.unit).unwrap();
        self.socket.recv_from(&mut [0u8; 64]).unwrap();
    }
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_lost_frame_times_out_without_reaching_the_device_and_the_next_send_recovers() {
    let (emulator, client) = open(2).await;
    let stats = client.bus_stats();

    emulator.drop_next_frames(0, 1);
    let error = client.send(ForceFan { value: true }).await.unwrap_err();

    assert!(matches!(error, Error::Timeout { .. }));
    assert_eq!(fans(&emulator), [false, true]);

    client.send(ForceFan { value: true }).await.unwrap();

    assert_eq!(stats.resets(), 2);
    assert_eq!(fans(&emulator), [true, true]);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn the_frame_after_a_lost_one_is_refused_with_a_sequence_mismatch() {
    let (emulator, client) = open(2).await;
    let stats = client.bus_stats();
    let before = telemetry(&client).await;
    let on = Frames::encode(client.geometry(), ForceFan { value: true }).unwrap();
    let nop = Frames::encode(client.geometry(), Nop).unwrap();

    emulator.drop_next_frames(1, 1);
    let lost = client.send_frame(nop.frame(0).unwrap()).await.unwrap();
    let refused = client.send_frame(on.frame(0).unwrap()).await.unwrap();

    let error = refused.await.unwrap_err();
    let Error::SeqMismatch {
        device,
        expected,
        got,
    } = error
    else {
        panic!("expected a sequence mismatch, got {error:?}");
    };
    assert_eq!(device, 1);
    assert_eq!(got, expected.wrapping_sub(2));
    assert!(matches!(lost.await, Err(Error::Timeout { .. })));
    assert_eq!(fans(&emulator), [true, false]);
    assert_eq!(stats.resets(), 1);

    client.send(ForceFan { value: true }).await.unwrap();

    assert_eq!(stats.resets(), 2);
    assert_eq!(fans(&emulator), [true, true]);
    let after = telemetry(&client).await;
    assert_eq!(after[0].seq_mismatch - before[0].seq_mismatch, 0);
    assert_eq!(after[1].seq_mismatch - before[1].seq_mismatch, 1);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_frame_delivered_twice_is_executed_once() {
    let (emulator, client) = open(2).await;
    let stats = client.bus_stats();

    let baseline = telemetry(&client).await;
    client.send(Nop).await.unwrap();
    let undisturbed = telemetry(&client).await;
    emulator.duplicate_next_frames(0, 1);
    client.send(Nop).await.unwrap();
    let duplicated = telemetry(&client).await;

    for device in 0..2 {
        assert_eq!(
            duplicated[device].processed - undisturbed[device].processed,
            undisturbed[device].processed - baseline[device].processed
        );
        assert_eq!(duplicated[device].seq_mismatch, 0);
    }
    assert_eq!(undisturbed[0].dedup, 0);
    assert_eq!(duplicated[0].dedup, 1);
    assert_eq!(duplicated[1].dedup, 0);
    assert_eq!(stats.resets(), 1);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_frame_answered_while_an_older_one_is_unanswered_still_completes() {
    let (emulator, client) = open(1).await;
    let nop = Frames::encode(client.geometry(), Nop).unwrap();
    let frame = nop.frame(0).unwrap();

    emulator.drop_next_replies(0, 1);
    let unanswered = client.send_frame(frame).await.unwrap();
    let answered = client.send_frame(frame).await.unwrap();

    assert!(answered.await.unwrap().check().is_ok());
    assert!(matches!(unanswered.await, Err(Error::Timeout { .. })));
    client.send(Nop).await.unwrap();
    assert_eq!(client.bus_stats().resets(), 2);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn an_unacknowledged_reset_fails_the_send_and_the_next_send_sends_it_again() {
    let (emulator, client) = open(1).await;
    let stats = client.bus_stats();

    emulator.drop_next_replies(0, 2);
    assert!(matches!(client.send(Nop).await, Err(Error::Timeout { .. })));
    assert!(matches!(client.send(Nop).await, Err(Error::Timeout { .. })));
    assert_eq!(stats.resets(), 2);

    client.send(Nop).await.unwrap();
    assert_eq!(stats.resets(), 3);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_reset_dropped_by_a_stalled_device_is_sent_again_once_the_device_runs() {
    let (emulator, client) = open(1).await;
    let stats = client.bus_stats();
    let before = telemetry(&client).await;
    let nop = Frames::encode(client.geometry(), Nop).unwrap();

    emulator.pause_main_loop(0);
    let mut queued = Vec::new();
    for _ in 0..MAX_INFLIGHT {
        queued.push(client.send_frame(nop.frame(0).unwrap()).await.unwrap());
    }
    for response in queued {
        assert!(matches!(response.await, Err(Error::Timeout { .. })));
    }
    assert!(matches!(client.send(Nop).await, Err(Error::Timeout { .. })));
    assert_eq!(stats.resets(), 2);

    emulator.resume_main_loop(0);
    client.send(Nop).await.unwrap();

    assert_eq!(stats.resets(), 3);
    let after = telemetry(&client).await;
    assert_eq!(after[0].fifo_drop - before[0].fifo_drop, 1);
    assert_eq!(after[0].seq_mismatch, before[0].seq_mismatch);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_frame_dropped_by_a_full_device_queue_desynchronizes_the_next_one() {
    let emulator = emulator(2);
    let client = open_with(&emulator, &option(&emulator), patient_config()).await;
    let stats = client.bus_stats();
    let before = telemetry(&client).await;
    let on = Frames::encode(client.geometry(), ForceFan { value: true }).unwrap();
    let nop = Frames::encode(client.geometry(), Nop).unwrap();

    let stranger = Stranger::of(&emulator, 1);
    emulator.pause_main_loop(1);
    stranger.occupy_a_queue_slot();
    let mut accepted = Vec::new();
    for _ in 1..MAX_INFLIGHT {
        accepted.push(client.send_frame(nop.frame(0).unwrap()).await.unwrap());
    }
    let dropped = client.send_frame(on.frame(0).unwrap()).await.unwrap();
    stranger.wait_until_everything_sent_was_received();
    emulator.drop_next_replies(1, 1);
    emulator.resume_main_loop(1);
    for response in accepted {
        assert!(response.await.unwrap().check().is_ok());
    }
    let refused = client.send_frame(on.frame(0).unwrap()).await.unwrap();

    let error = refused.await.unwrap_err();
    let Error::SeqMismatch {
        device,
        expected,
        got,
    } = error
    else {
        panic!("expected a sequence mismatch, got {error:?}");
    };
    assert_eq!(device, 1);
    assert_eq!(got, expected.wrapping_sub(2));
    assert_eq!(fans(&emulator), [true, false]);
    assert_eq!(stats.resets(), 1);

    client.send(ForceFan { value: true }).await.unwrap();

    assert!(matches!(dropped.await, Err(Error::Timeout { .. })));
    assert_eq!(stats.resets(), 2);
    assert_eq!(fans(&emulator), [true, true]);
    let after = telemetry(&client).await;
    assert_eq!(after[0].fifo_drop, before[0].fifo_drop);
    assert_eq!(after[1].fifo_drop - before[1].fifo_drop, 1);
    assert_eq!(after[1].seq_mismatch - before[1].seq_mismatch, 2);
    client.close().await.unwrap();
}
