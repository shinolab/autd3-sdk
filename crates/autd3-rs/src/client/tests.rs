use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use crate::datagram::Datagram;
use crate::error::Error;
use crate::firmware_version::FirmwareVersion;
use crate::geometry::{Autd3, Geometry};
use crate::link::{CycleOutcome, Link};
use crate::operation::{Operation, XOR_HASH_MAX_DATA_LEN, XorHashCmd};
use crate::protocol::{
    Cmd, MAX_IN_FLIGHT, MODE_FIFO, MODE_LOW_LATENCY, PAYLOAD_BYTES, RX_FRAME_BYTES, TX_FRAME_BYTES,
    TxFrame,
};

use super::{Client, ClientConfig};

fn geometry(n: usize) -> Geometry {
    Geometry::new((0..n).map(|_| Autd3::default()).collect())
}

async fn xor_hash(client: &Client, cmd: &XorHashCmd) -> Result<(), Error> {
    let datagrams = client.datagram_builder().push(cmd).build()?;
    for frame in &datagrams {
        client.send_checked(frame).await?;
    }
    Ok(())
}

struct LoopbackLink {
    slaves: Vec<Arc<StdMutex<Slave>>>,
}

struct Slave {
    expected_seq: u8,
    ack: u8,
    data: u8,
    fw_version_major: u8,
    fw_version_minor: u8,
    fw_version_patch: u8,
    error_detail: u8,
    fpga_state: u8,
    drop_next: u32,
    stale_for_next: u32,
    sent_log: Vec<(u8, Cmd)>,
    xor_hash_total_sleep_ms: u32,
    mode: u8,
}

impl Slave {
    fn new() -> Self {
        Self {
            expected_seq: 0,
            ack: 0xFF,
            data: 0,
            fw_version_major: 0,
            fw_version_minor: 0,
            fw_version_patch: 0,
            error_detail: 0,
            fpga_state: 0,
            drop_next: 0,
            stale_for_next: 0,
            sent_log: Vec::new(),
            xor_hash_total_sleep_ms: 0,
            mode: MODE_FIFO,
        }
    }
}

const ERR_INVALID_PAYLOAD: u8 = 0x02;
const ERR_INVALID_DATA: u8 = 0x03;

fn handle_xor_hash(payload: &[u8; PAYLOAD_BYTES], slave: &mut Slave) -> u8 {
    let sleep_ms = u16::from_le_bytes([payload[0], payload[1]]);
    let data_len = u16::from_le_bytes([payload[2], payload[3]]) as usize;
    if data_len > XOR_HASH_MAX_DATA_LEN {
        slave.error_detail = ERR_INVALID_PAYLOAD;
        return ERR_INVALID_PAYLOAD;
    }
    slave.xor_hash_total_sleep_ms = slave
        .xor_hash_total_sleep_ms
        .saturating_add(u32::from(sleep_ms));
    let mut h: u8 = 0;
    for b in &payload[4..4 + data_len] {
        h ^= *b;
    }
    if h != 0 {
        slave.error_detail = ERR_INVALID_DATA;
        ERR_INVALID_DATA
    } else {
        0
    }
}

fn slave_cycle(
    slave: &mut Slave,
    tx: &[u8; TX_FRAME_BYTES],
    rx: &mut [u8; RX_FRAME_BYTES],
) -> bool {
    let parsed = TxFrame::parse(tx).expect("loopback only sees known cmds");
    slave.sent_log.push((parsed.seq.get(), parsed.cmd));

    if parsed.cmd == Cmd::Reset {
        slave.expected_seq = 0;
        slave.ack = 0xFF;
        slave.data = 0;
        *rx = [slave.ack, slave.data];
        return true;
    }

    if slave.stale_for_next > 0 {
        slave.stale_for_next -= 1;
        *rx = [slave.ack, slave.data];
        return false;
    }

    if parsed.seq.get() != slave.expected_seq {
        *rx = [slave.ack, slave.data];
        return true;
    }

    if slave.drop_next > 0 {
        slave.drop_next -= 1;
        *rx = [slave.ack, slave.data];
        return true;
    }

    slave.expected_seq = slave.expected_seq.wrapping_add(1);
    let data = match parsed.cmd {
        Cmd::XorHash => handle_xor_hash(&parsed.payload, slave),
        Cmd::ReadCpuFwVersionMajor => slave.fw_version_major,
        Cmd::ReadCpuFwVersionMinor => slave.fw_version_minor,
        Cmd::ReadCpuFwVersionPatch => slave.fw_version_patch,
        Cmd::ReadErrorDetail => slave.error_detail,
        Cmd::ReadFpgaState => slave.fpga_state,
        Cmd::WritePatternBuffer
        | Cmd::WritePatternCompressed
        | Cmd::WriteModulationBuffer
        | Cmd::ConfigModulation
        | Cmd::ConfigPattern
        | Cmd::ChangePatternBank
        | Cmd::ChangeModulationBank
        | Cmd::SetSilencer
        | Cmd::SetPhaseCorrection
        | Cmd::SetOutputMask
        | Cmd::SetPulseWidthTable
        | Cmd::EmulateGpioIn
        | Cmd::SetGpioOut
        | Cmd::ForceFan
        | Cmd::Synchronize
        | Cmd::Clear
        | Cmd::Nop => 0,
        Cmd::SetMode => {
            slave.mode = parsed.payload[0];
            0
        }
        Cmd::Reset => unreachable!(),
    };
    slave.ack = parsed.seq.get();
    slave.data = data;
    *rx = [slave.ack, slave.data];
    true
}

impl Link for LoopbackLink {
    type Error = std::convert::Infallible;
    type Checker = crate::link::ConstStateChecker;

    fn num_devices(&self) -> usize {
        self.slaves.len()
    }

    fn state_checker(&self) -> Self::Checker {
        crate::link::ConstStateChecker::new(self.slaves.len())
    }

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Self::Error> {
        let mut rx_valid = true;
        for ((tx, rx), slave) in tx.iter().zip(rx.iter_mut()).zip(&self.slaves) {
            let mut s = slave.lock().unwrap();
            rx_valid &= slave_cycle(&mut s, tx, rx);
        }
        Ok(CycleOutcome { rx_valid })
    }
}

fn slaves_pair(n: usize) -> (LoopbackLink, Vec<Arc<StdMutex<Slave>>>) {
    let slaves: Vec<_> = (0..n)
        .map(|_| Arc::new(StdMutex::new(Slave::new())))
        .collect();
    (
        LoopbackLink {
            slaves: slaves.clone(),
        },
        slaves,
    )
}

fn slave_pair() -> (LoopbackLink, Arc<StdMutex<Slave>>) {
    let (link, mut slaves) = slaves_pair(1);
    (link, slaves.pop().expect("one slave"))
}

async fn open_client() -> (Client, Arc<StdMutex<Slave>>) {
    let (link, slave) = slave_pair();
    let client = Client::open(&geometry(1), link, ClientConfig::default())
        .await
        .unwrap();
    (client, slave)
}

#[tokio::test]
async fn xor_hash_with_checksum_returns_ok() {
    let (client, slave) = open_client().await;
    let cmd = XorHashCmd::with_checksum(3, vec![0x01, 0x02, 0x04, 0x08]);
    xor_hash(&client, &cmd).await.unwrap();

    let s = slave.lock().unwrap();
    assert_eq!(s.ack, 2);
    assert_eq!(s.expected_seq, 3);
    assert_eq!(s.xor_hash_total_sleep_ms, 3);
    assert_eq!(s.error_detail, 0);
}

#[tokio::test]
async fn xor_hash_with_non_zero_xor_returns_device_error() {
    let (client, _slave) = open_client().await;
    let cmd = XorHashCmd {
        sleep_ms: 0,
        data: vec![0xAA],
    };
    let err = xor_hash(&client, &cmd).await.unwrap_err();
    match err {
        Error::DeviceError { device, code } => {
            assert_eq!(device, 0);
            assert_eq!(code, ERR_INVALID_DATA);
        }
        other => panic!("expected DeviceError, got {other:?}"),
    }
}

#[tokio::test]
async fn xor_hash_rejects_oversize_data_locally() {
    let (client, _slave) = open_client().await;
    let cmd = XorHashCmd {
        sleep_ms: 0,
        data: vec![0; XOR_HASH_MAX_DATA_LEN + 1],
    };
    let err = xor_hash(&client, &cmd).await.unwrap_err();
    assert!(matches!(err, Error::InvalidPayload(_)));
}

#[tokio::test]
async fn read_firmware_version_returns_full_triplet() {
    let (client, slave) = open_client().await;
    {
        let mut s = slave.lock().unwrap();
        s.fw_version_major = 1;
        s.fw_version_minor = 2;
        s.fw_version_patch = 3;
    }
    let v = client.read_firmware_version().await.unwrap();
    assert_eq!(
        v,
        vec![FirmwareVersion {
            major: 1,
            minor: 2,
            patch: 3,
        }]
    );
    assert_eq!(v[0].to_string(), "1.2.3");
}

#[tokio::test]
async fn read_error_detail_returns_error_code() {
    let (client, slave) = open_client().await;
    slave.lock().unwrap().error_detail = 0x7A;
    let e = client.read_error_detail().await.unwrap();
    assert_eq!(e, vec![0x7A]);
}

#[tokio::test]
async fn xor_hash_error_is_observable_via_read_error_detail() {
    let (client, _slave) = open_client().await;
    let bad = XorHashCmd {
        sleep_ms: 0,
        data: vec![0xAA],
    };
    let _ = xor_hash(&client, &bad).await;
    let detail = client.read_error_detail().await.unwrap();
    assert_eq!(detail, vec![ERR_INVALID_DATA]);
}

#[tokio::test]
async fn read_is_exclusive_and_correct_under_concurrent_writes() {
    let (link, slaves) = slaves_pair(2);
    {
        let mut s0 = slaves[0].lock().unwrap();
        s0.fw_version_major = 0xA0;
        s0.fw_version_minor = 0xA1;
        s0.fw_version_patch = 0xA2;
        let mut s1 = slaves[1].lock().unwrap();
        s1.fw_version_major = 0xB0;
        s1.fw_version_minor = 0xB1;
        s1.fw_version_patch = 0xB2;
    }
    let client = Arc::new(
        Client::open(&geometry(2), link, ClientConfig::default())
            .await
            .unwrap(),
    );

    let writer = {
        let client = Arc::clone(&client);
        tokio::spawn(async move {
            for _ in 0..50 {
                xor_hash(&client, &XorHashCmd::with_checksum(0, vec![0x01, 0x02]))
                    .await
                    .unwrap();
            }
        })
    };

    let expected = vec![
        FirmwareVersion {
            major: 0xA0,
            minor: 0xA1,
            patch: 0xA2,
        },
        FirmwareVersion {
            major: 0xB0,
            minor: 0xB1,
            patch: 0xB2,
        },
    ];
    for _ in 0..10 {
        assert_eq!(client.read_firmware_version().await.unwrap(), expected);
    }
    writer.await.unwrap();
}

#[tokio::test]
async fn multi_device_per_device_payloads_yield_per_device_results() {
    let (link, _slaves) = slaves_pair(2);
    let client = Client::open(&geometry(2), link, ClientConfig::default())
        .await
        .unwrap();

    let mut ok_payload = [0u8; PAYLOAD_BYTES];
    XorHashCmd::with_checksum(0, vec![0x01, 0x02])
        .encode(0, 0, &mut ok_payload)
        .unwrap();
    let ok = Datagram {
        cmd: Cmd::XorHash,
        payload: ok_payload,
    };

    let mut bad_payload = [0u8; PAYLOAD_BYTES];
    bad_payload[2] = 1;
    bad_payload[4] = 0xAA;

    let fut = client
        .send_datagrams(&[
            ok,
            Datagram {
                cmd: Cmd::XorHash,
                payload: bad_payload,
            },
        ])
        .await
        .unwrap();
    let resp = fut.await.unwrap();
    assert_eq!(resp.data, vec![0, ERR_INVALID_DATA]);
}

#[tokio::test]
async fn multi_device_xor_hash_reports_failing_device_index() {
    let (link, slaves) = slaves_pair(2);
    let client = Client::open(&geometry(2), link, ClientConfig::default())
        .await
        .unwrap();
    let bad = XorHashCmd {
        sleep_ms: 0,
        data: vec![0xAA],
    };
    let err = xor_hash(&client, &bad).await.unwrap_err();
    match err {
        Error::DeviceError { device, code } => {
            assert_eq!(device, 0);
            assert_eq!(code, ERR_INVALID_DATA);
        }
        other => panic!("expected DeviceError, got {other:?}"),
    }
    for slave in &slaves {
        assert_eq!(slave.lock().unwrap().error_detail, ERR_INVALID_DATA);
    }
}

#[tokio::test]
async fn multi_device_skip_on_one_device_recovers_via_resync() {
    let (link, slaves) = slaves_pair(2);
    slaves[1].lock().unwrap().fw_version_major = 0xB1;
    slaves[0].lock().unwrap().fw_version_major = 0xB0;
    let client = Client::open(
        &geometry(2),
        link,
        ClientConfig {
            timeout_cycles: 10,
            max_inflight: NonZeroUsize::new(16).unwrap(),
            send_interval_cycles: NonZeroU32::new(1).unwrap(),
            max_resync_rounds: NonZeroU32::new(8).unwrap(),
            low_latency: false,
            reset_resend_cycles: 2,
            rt_priority: None,
            rt_affinity: None,
            validate_state: true,
        },
    )
    .await
    .unwrap();
    slaves[1].lock().unwrap().drop_next = 1;

    let mut futs = Vec::new();
    for _ in 0..8 {
        futs.push(
            client
                .send_broadcast(&Datagram::no_payload(Cmd::ReadCpuFwVersionMajor))
                .await
                .unwrap(),
        );
    }
    for f in futs {
        assert_eq!(
            f.await.unwrap().data,
            vec![0xB0, 0xB1],
            "resync must recover as success with per-device data"
        );
    }
    assert_eq!(slaves[0].lock().unwrap().expected_seq, 10);
    assert_eq!(slaves[1].lock().unwrap().expected_seq, 10);
}

#[tokio::test]
async fn send_rejects_wrong_datagram_count() {
    let (link, _slaves) = slaves_pair(2);
    let client = Client::open(&geometry(2), link, ClientConfig::default())
        .await
        .unwrap();
    let err = client
        .send_datagrams(&[Datagram::no_payload(Cmd::ReadCpuFwVersionMajor)])
        .await
        .err()
        .expect("send with wrong datagram count must fail");
    assert!(matches!(err, Error::InvalidPayload(_)));
}

#[tokio::test]
async fn handshake_sends_two_resets_with_seqs_zero_then_one() {
    let (_client, slave) = open_client().await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    let s = slave.lock().unwrap();
    assert!(s.sent_log.len() >= 2);
    assert_eq!(s.sent_log[0], (0, Cmd::Reset));
    assert_eq!(s.sent_log[1], (1, Cmd::Reset));
    assert!(s.sent_log.contains(&(0, Cmd::Clear)));
    assert!(s.sent_log.contains(&(1, Cmd::Synchronize)));
}

#[tokio::test]
async fn low_latency_handshake_switches_slave_mode_and_continues_traffic() {
    let (link, slave) = slave_pair();
    let config = ClientConfig {
        low_latency: true,
        ..ClientConfig::default()
    };
    let client = Client::open(&geometry(1), link, config).await.unwrap();
    {
        let s = slave.lock().unwrap();
        assert_eq!(s.mode, MODE_LOW_LATENCY, "slave must switch to low-latency");
        assert!(s.sent_log.contains(&(0, Cmd::SetMode)));
        assert_eq!(s.expected_seq, 3);
    }
    xor_hash(&client, &XorHashCmd::with_checksum(0, vec![]))
        .await
        .unwrap();
    assert_eq!(slave.lock().unwrap().expected_seq, 4);
}

#[tokio::test]
async fn default_config_leaves_slave_in_fifo_mode() {
    let (_client, slave) = open_client().await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    let s = slave.lock().unwrap();
    assert_eq!(s.mode, MODE_FIFO);
    assert!(!s.sent_log.iter().any(|(_, cmd)| *cmd == Cmd::SetMode));
}

#[tokio::test]
async fn handshake_resets_slave_proto_state() {
    let (link, slave) = slave_pair();
    {
        let mut s = slave.lock().unwrap();
        s.expected_seq = 42;
        s.ack = 41;
    }
    let client = Client::open(&geometry(1), link, ClientConfig::default())
        .await
        .unwrap();
    {
        let s = slave.lock().unwrap();
        assert_eq!(s.expected_seq, 2);
        assert_eq!(s.ack, 1);
    }
    xor_hash(&client, &XorHashCmd::with_checksum(0, vec![]))
        .await
        .unwrap();
    assert_eq!(slave.lock().unwrap().expected_seq, 3);
}

#[tokio::test]
async fn two_stage_await_resolves_in_order() {
    let (client, slave) = open_client().await;
    {
        let mut s = slave.lock().unwrap();
        s.fw_version_major = 0xAA;
        s.fw_version_minor = 0xBB;
    }
    let f1 = client
        .send_broadcast(&Datagram::no_payload(Cmd::ReadCpuFwVersionMajor))
        .await
        .unwrap();
    let f2 = client
        .send_broadcast(&Datagram::no_payload(Cmd::ReadCpuFwVersionMinor))
        .await
        .unwrap();
    let r1 = f1.await.unwrap();
    let r2 = f2.await.unwrap();
    assert_eq!(r1.data, vec![0xAA]);
    assert_eq!(r2.data, vec![0xBB]);
}

#[tokio::test]
async fn pipeline_continues_after_device_error_in_the_middle() {
    let (client, slave) = open_client().await;
    slave.lock().unwrap().fw_version_major = 0x42;

    let mut bad_payload = [0u8; PAYLOAD_BYTES];
    bad_payload[2] = 1;
    bad_payload[4] = 0xAA;

    let f1 = client
        .send_broadcast(&Datagram::no_payload(Cmd::ReadCpuFwVersionMajor))
        .await
        .unwrap();
    let f2 = client
        .send_broadcast(&Datagram {
            cmd: Cmd::XorHash,
            payload: bad_payload,
        })
        .await
        .unwrap();
    let f3 = client
        .send_broadcast(&Datagram::no_payload(Cmd::ReadCpuFwVersionMajor))
        .await
        .unwrap();

    assert_eq!(f1.await.unwrap().data, vec![0x42]);
    let mid = f2.await.unwrap();
    assert_eq!(mid.data, vec![ERR_INVALID_DATA]);
    assert_eq!(f3.await.unwrap().data, vec![0x42]);
}

#[tokio::test]
async fn streaming_skip_recovers_via_resync_without_timeout() {
    let (link, slave) = slave_pair();
    slave.lock().unwrap().fw_version_major = 0xAB;
    let client = Client::open(
        &geometry(1),
        link,
        ClientConfig {
            timeout_cycles: 10,
            max_inflight: NonZeroUsize::new(16).unwrap(),
            send_interval_cycles: NonZeroU32::new(1).unwrap(),
            max_resync_rounds: NonZeroU32::new(8).unwrap(),
            low_latency: false,
            reset_resend_cycles: 2,
            rt_priority: None,
            rt_affinity: None,
            validate_state: true,
        },
    )
    .await
    .unwrap();
    slave.lock().unwrap().drop_next = 1;

    let mut futs = Vec::new();
    for _ in 0..8 {
        futs.push(
            client
                .send_broadcast(&Datagram::no_payload(Cmd::ReadCpuFwVersionMajor))
                .await
                .unwrap(),
        );
    }
    for f in futs {
        assert_eq!(
            f.await.unwrap().data,
            vec![0xAB],
            "resync must recover as success"
        );
    }
    assert_eq!(slave.lock().unwrap().expected_seq, 10);
}

#[tokio::test]
async fn dead_link_gives_up_whole_window_in_bounded_time() {
    let (link, slave) = slave_pair();
    let client = Client::open(
        &geometry(1),
        link,
        ClientConfig {
            timeout_cycles: 5,
            max_inflight: NonZeroUsize::new(8).unwrap(),
            send_interval_cycles: NonZeroU32::new(1).unwrap(),
            max_resync_rounds: NonZeroU32::new(3).unwrap(),
            low_latency: false,
            reset_resend_cycles: 2,
            rt_priority: None,
            rt_affinity: None,
            validate_state: true,
        },
    )
    .await
    .unwrap();
    slave.lock().unwrap().drop_next = u32::MAX;

    let mut futs = Vec::new();
    for _ in 0..3 {
        futs.push(
            client
                .send_broadcast(&Datagram::no_payload(Cmd::ReadCpuFwVersionMajor))
                .await
                .unwrap(),
        );
    }
    for f in futs {
        assert!(
            matches!(f.await, Err(Error::Timeout { .. })),
            "dead link must surface Timeout, not hang",
        );
    }
}

#[tokio::test]
async fn stale_cycles_block_false_positive_ack_match() {
    let (client, slave) = open_client().await;
    {
        let mut s = slave.lock().unwrap();
        s.ack = 0;
        s.data = 0;
        s.stale_for_next = u32::MAX;
    }
    let err = xor_hash(&client, &XorHashCmd::with_checksum(0, vec![]))
        .await
        .unwrap_err();
    match err {
        Error::Timeout { cycles } => assert_eq!(cycles, 10),
        other => panic!("expected Timeout, got {other:?}"),
    }
}

#[tokio::test]
async fn recovers_after_transient_stale_cycles() {
    let (client, slave) = open_client().await;
    slave.lock().unwrap().stale_for_next = 3;
    xor_hash(&client, &XorHashCmd::with_checksum(0, vec![]))
        .await
        .expect("xor_hash should recover after the stale burst");
    let s = slave.lock().unwrap();
    assert_eq!(s.expected_seq, 3);
    assert_eq!(s.ack, 2);
}

fn seq0_reset_count(slave: &Arc<StdMutex<Slave>>) -> usize {
    slave
        .lock()
        .unwrap()
        .sent_log
        .iter()
        .filter(|(seq, cmd)| *cmd == Cmd::Reset && *seq == 0)
        .count()
}

#[tokio::test]
async fn inflight_held_across_stale_recovers_without_reset() {
    let (link, slave) = slave_pair();
    slave.lock().unwrap().fw_version_major = 0xAB;
    let client = Client::open(&geometry(1), link, ClientConfig::default())
        .await
        .unwrap();
    slave.lock().unwrap().stale_for_next = 40;

    let fut = client
        .send_broadcast(&Datagram::no_payload(Cmd::ReadCpuFwVersionMajor))
        .await
        .unwrap();
    assert_eq!(
        fut.await.unwrap().data,
        vec![0xAB],
        "held in-flight must recover after the stale burst, not time out"
    );
    let s = slave.lock().unwrap();
    assert_eq!(
        s.expected_seq, 3,
        "Clear(seq0) + Synchronize(seq1) + one command, each once"
    );
    assert_eq!(s.ack, 2);
    drop(s);
    assert_eq!(
        seq0_reset_count(&slave),
        1,
        "no Reset escalation when the held front still matches expected_seq"
    );
}

#[tokio::test]
async fn streaming_holds_window_across_stale_and_recovers() {
    let (link, slave) = slave_pair();
    slave.lock().unwrap().fw_version_major = 0xAB;
    let client = Client::open(
        &geometry(1),
        link,
        ClientConfig {
            timeout_cycles: 10,
            max_inflight: NonZeroUsize::new(8).unwrap(),
            send_interval_cycles: NonZeroU32::new(1).unwrap(),
            max_resync_rounds: NonZeroU32::new(8).unwrap(),
            low_latency: false,
            reset_resend_cycles: 2,
            rt_priority: None,
            rt_affinity: None,
            validate_state: true,
        },
    )
    .await
    .unwrap();
    slave.lock().unwrap().stale_for_next = 30;

    let mut futs = Vec::new();
    for _ in 0..8 {
        futs.push(
            client
                .send_broadcast(&Datagram::no_payload(Cmd::ReadCpuFwVersionMajor))
                .await
                .unwrap(),
        );
    }
    for f in futs {
        assert_eq!(
            f.await.unwrap().data,
            vec![0xAB],
            "every held in-flight must recover after the stale burst"
        );
    }
    assert_eq!(slave.lock().unwrap().expected_seq, 10);
    assert_eq!(seq0_reset_count(&slave), 1, "no Reset escalation needed");
}

#[tokio::test]
async fn frozen_ahead_desync_recovers_via_reset_resync() {
    let (link, slave) = slave_pair();
    slave.lock().unwrap().fw_version_major = 0xCD;
    let client = Client::open(&geometry(1), link, ClientConfig::default())
        .await
        .unwrap();
    slave.lock().unwrap().expected_seq = 200;

    let fut = client
        .send_broadcast(&Datagram::no_payload(Cmd::ReadCpuFwVersionMajor))
        .await
        .unwrap();
    assert_eq!(
        fut.await.unwrap().data,
        vec![0xCD],
        "Reset re-sync must recover the desync instead of waiting for SEQ wraparound"
    );
    assert!(
        seq0_reset_count(&slave) > 1,
        "expected a Reset escalation beyond the single handshake seq-0 reset"
    );
}

#[tokio::test]
async fn close_resolves_pending_with_rt_closed() {
    let (client, slave) = open_client().await;
    slave.lock().unwrap().drop_next = u32::MAX;
    let f = client
        .send_broadcast(&Datagram::no_payload(Cmd::ReadCpuFwVersionMajor))
        .await
        .unwrap();
    client.close().await.unwrap();
    let err = f.await.unwrap_err();
    assert!(
        matches!(err, Error::RtClosed) || matches!(err, Error::Timeout { .. }),
        "expected RtClosed or Timeout, got {err:?}",
    );
}

#[tokio::test]
async fn open_rejects_oversize_max_inflight() {
    let (link, _slave) = slave_pair();
    let res = Client::open(
        &geometry(1),
        link,
        ClientConfig {
            timeout_cycles: 10,
            max_inflight: NonZeroUsize::new(MAX_IN_FLIGHT + 1).unwrap(),
            send_interval_cycles: NonZeroU32::new(1).unwrap(),
            max_resync_rounds: NonZeroU32::new(8).unwrap(),
            low_latency: false,
            reset_resend_cycles: 2,
            rt_priority: None,
            rt_affinity: None,
            validate_state: true,
        },
    )
    .await;
    assert!(matches!(res, Err(Error::InvalidPayload(_))));
}

#[tokio::test]
async fn open_rejects_zero_devices() {
    let (link, _slaves) = slaves_pair(0);
    let res = Client::open(&geometry(0), link, ClientConfig::default()).await;
    assert!(matches!(res, Err(Error::InvalidPayload(_))));
}

#[tokio::test]
async fn commands_still_succeed_with_send_interval_above_one() {
    let (link, slave) = slave_pair();
    {
        let mut s = slave.lock().unwrap();
        s.fw_version_major = 0x11;
        s.fw_version_minor = 0x22;
        s.fw_version_patch = 0x33;
    }
    let client = Client::open(
        &geometry(1),
        link,
        ClientConfig {
            timeout_cycles: 10,
            max_inflight: NonZeroUsize::new(8).unwrap(),
            send_interval_cycles: NonZeroU32::new(3).unwrap(),
            max_resync_rounds: NonZeroU32::new(8).unwrap(),
            low_latency: false,
            reset_resend_cycles: 2,
            rt_priority: None,
            rt_affinity: None,
            validate_state: true,
        },
    )
    .await
    .unwrap();
    let v = client.read_firmware_version().await.unwrap();
    assert_eq!(
        v,
        vec![FirmwareVersion {
            major: 0x11,
            minor: 0x22,
            patch: 0x33,
        }]
    );
}

#[tokio::test]
async fn build_rejects_too_fast_pattern_under_strict_silencer() {
    use crate::operation::{ConfigPattern, SetSilencer};
    use crate::value::{PatternBank, PatternDataType};

    let (client, _slave) = open_client().await;
    let mut builder = client.datagram_builder();
    builder.push(SetSilencer::default()).push(ConfigPattern {
        bank: PatternBank::B0,
        divider: 1,
        size: 1,
        data_type: PatternDataType::Raw,
    });
    match builder.build().unwrap_err() {
        Error::SilencerConstraint {
            device,
            axis,
            completion_steps,
            sampling_div,
        } => {
            assert_eq!(device, 0);
            assert_eq!(axis, crate::SilencerAxis::Intensity);
            assert_eq!(completion_steps, 10);
            assert_eq!(sampling_div, 1);
        }
        other => panic!("expected SilencerConstraint, got {other:?}"),
    }
}

#[tokio::test]
async fn build_rejects_strict_silencer_when_active_sampling_too_fast() {
    use crate::common::ULTRASOUND_PERIOD;
    use crate::operation::{ConfigModulation, FixedCompletionTime, SetSilencer};
    use crate::value::ModulationBank;

    let (client, _slave) = open_client().await;
    let mut builder = client.datagram_builder();
    builder
        .push(ConfigModulation {
            bank: ModulationBank::B0,
            divider: 5,
            size: 1,
        })
        .push(SetSilencer::new(FixedCompletionTime {
            intensity: ULTRASOUND_PERIOD * 8,
            phase: ULTRASOUND_PERIOD * 40,
            strict_mode: true,
        }));
    assert!(matches!(
        builder.build().unwrap_err(),
        Error::SilencerConstraint {
            axis: crate::SilencerAxis::Intensity,
            completion_steps: 8,
            sampling_div: 5,
            ..
        }
    ));
}

#[tokio::test]
async fn opt_out_disables_precheck() {
    use crate::operation::{ConfigPattern, SetSilencer};
    use crate::value::{PatternBank, PatternDataType};

    let (link, _slave) = slave_pair();
    let config = ClientConfig {
        validate_state: false,
        ..ClientConfig::default()
    };
    let client = Client::open(&geometry(1), link, config).await.unwrap();
    let mut builder = client.datagram_builder();
    builder.push(SetSilencer::default()).push(ConfigPattern {
        bank: PatternBank::B0,
        divider: 1,
        size: 1,
        data_type: PatternDataType::Raw,
    });
    assert!(
        builder.build().is_ok(),
        "opt-out must skip the local pre-check and defer to the CPU guard"
    );
}

#[tokio::test]
async fn desync_after_send_failure_stops_precheck() {
    use crate::operation::{ConfigPattern, SetSilencer};
    use crate::value::{PatternBank, PatternDataType};

    let too_fast = |client: &Client| {
        let mut builder = client.datagram_builder();
        builder.push(ConfigPattern {
            bank: PatternBank::B0,
            divider: 1,
            size: 1,
            data_type: PatternDataType::Raw,
        });
        builder.build()
    };

    let (client, slave) = open_client().await;

    let datagrams = client
        .datagram_builder()
        .push(SetSilencer::default())
        .build()
        .unwrap();
    for frame in &datagrams {
        client.send_checked(frame).await.unwrap();
    }

    assert!(matches!(
        too_fast(&client),
        Err(Error::SilencerConstraint { .. })
    ));

    slave.lock().unwrap().stale_for_next = u32::MAX;
    assert!(matches!(
        xor_hash(&client, &XorHashCmd::with_checksum(0, vec![]))
            .await
            .unwrap_err(),
        Error::Timeout { .. }
    ));

    assert!(
        too_fast(&client).is_ok(),
        "desynced mirror must stop pre-checking until the next Clear/reopen"
    );
}

#[tokio::test]
async fn build_rejects_per_device_group_under_strict_silencer() {
    use crate::operation::{ConfigModulation, SetSilencer};
    use crate::value::ModulationBank;

    let (link, _slaves) = slaves_pair(2);
    let client = Client::open(&geometry(2), link, ClientConfig::default())
        .await
        .unwrap();

    let datagrams = client
        .datagram_builder()
        .push(SetSilencer::default())
        .build()
        .unwrap();
    for frame in &datagrams {
        client.send_checked(frame).await.unwrap();
    }

    let mut builder = client.datagram_builder();
    builder.push_each(|device| {
        Some(ConfigModulation {
            bank: ModulationBank::B0,
            divider: if device == 0 { 5 } else { 20 },
            size: 1,
        })
    });
    match builder.build().unwrap_err() {
        Error::SilencerConstraint { device, .. } => assert_eq!(device, 0),
        other => panic!("expected SilencerConstraint on device 0, got {other:?}"),
    }
}

#[tokio::test]
async fn separate_builders_share_committed_mirror_state() {
    use crate::operation::{ConfigPattern, SetSilencer};
    use crate::value::{PatternBank, PatternDataType};

    let (client, _slave) = open_client().await;
    client
        .datagram_builder()
        .push(SetSilencer::default())
        .build()
        .unwrap();
    let mut b2 = client.datagram_builder();
    b2.push(ConfigPattern {
        bank: PatternBank::B0,
        divider: 1,
        size: 1,
        data_type: PatternDataType::Raw,
    });
    assert!(matches!(b2.build(), Err(Error::SilencerConstraint { .. })));
}
