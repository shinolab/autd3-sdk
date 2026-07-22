use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use crate::commands::operation::{Distribution, Nop, Operation};
use crate::datagram::Datagram;
use crate::error::Error;
use crate::firmware_version::{FirmwareVersion, Version};
use crate::geometry::Device;
use crate::geometry::{Autd3, Geometry};
use crate::link::{CycleOutcome, Link};
use crate::protocol::{Cmd, MAX_IN_FLIGHT, PAYLOAD_BYTES, RX_FRAME_BYTES, TX_FRAME_BYTES, TxFrame};

use crate::telemetry::Telemetry;
use autd3_cpu_wire::Mode;

use super::{Client, ClientConfig};
use crate::RtSchedulePolicy;

fn geometry(n: usize) -> Geometry {
    Geometry::new((0..n).map(|_| Autd3::default()).collect())
}

const FAIL_MARKER: u8 = 0xAA;

struct FailingCmd;

impl Operation for FailingCmd {
    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(&self, _device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        out[0] = FAIL_MARKER;
        Ok(Cmd::Nop)
    }
}

fn failing_payload() -> [u8; PAYLOAD_BYTES] {
    let mut p = [0u8; PAYLOAD_BYTES];
    p[0] = FAIL_MARKER;
    p
}

async fn send_op<O: Operation + 'static>(client: &Client, op: O) -> Result<(), Error> {
    let datagrams = client.datagram_builder().push(op).build()?;
    for frame in &datagrams {
        client.send_checked(frame).await?;
    }
    Ok(())
}

async fn send_nop(client: &Client) -> Result<(), Error> {
    send_op(client, Nop).await
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
    fpga_version_major: u8,
    fpga_version_minor: u8,
    fpga_version_patch: u8,
    supports_fpga_version: bool,
    error_detail: u8,
    fpga_state: u8,
    fpga_functions: u8,
    supports_fpga_functions: bool,
    telemetry: [u8; 6],
    sync_resync_count: u8,
    muted: bool,
    drop_next: u32,
    stale_for_next: u32,
    sent_log: Vec<(u8, Cmd)>,
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
            fpga_version_major: 0,
            fpga_version_minor: 0,
            fpga_version_patch: 0,
            supports_fpga_version: true,
            error_detail: 0,
            fpga_state: 0,
            fpga_functions: 0,
            supports_fpga_functions: true,
            telemetry: [0; 6],
            sync_resync_count: 0,
            muted: false,
            drop_next: 0,
            stale_for_next: 0,
            sent_log: Vec::new(),
            mode: Mode::Fifo.as_u8(),
        }
    }
}

const ERR_UNKNOWN_CMD: u8 = 0x01;
const ERR_INVALID_PAYLOAD: u8 = 0x02;
const ERR_INVALID_DATA: u8 = 0x03;

fn handle_nop(payload: &[u8; PAYLOAD_BYTES], slave: &mut Slave) -> u8 {
    if payload[0] == FAIL_MARKER {
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

    if parsed.cmd == Cmd::Stop {
        slave.muted = true;
        slave.expected_seq = parsed.seq.get().wrapping_add(1);
        slave.ack = parsed.seq.get();
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
        Cmd::Nop => handle_nop(&parsed.payload, slave),
        Cmd::ReadCpuFwVersionMajor => slave.fw_version_major,
        Cmd::ReadCpuFwVersionMinor => slave.fw_version_minor,
        Cmd::ReadCpuFwVersionPatch => slave.fw_version_patch,
        Cmd::ReadFpgaFwVersionMajor | Cmd::ReadFpgaFwVersionMinor | Cmd::ReadFpgaFwVersionPatch
            if !slave.supports_fpga_version =>
        {
            slave.error_detail = ERR_UNKNOWN_CMD;
            ERR_UNKNOWN_CMD
        }
        Cmd::ReadFpgaFwVersionMajor => slave.fpga_version_major,
        Cmd::ReadFpgaFwVersionMinor => slave.fpga_version_minor,
        Cmd::ReadFpgaFwVersionPatch => slave.fpga_version_patch,
        Cmd::ReadErrorDetail => slave.error_detail,
        Cmd::ReadFpgaState => slave.fpga_state,
        Cmd::ReadFpgaFunctions if !slave.supports_fpga_functions => {
            slave.error_detail = ERR_UNKNOWN_CMD;
            ERR_UNKNOWN_CMD
        }
        Cmd::ReadFpgaFunctions => slave.fpga_functions,
        Cmd::ReadTelemetry if parsed.payload[0] == Telemetry::SyncResync.as_u8() => {
            slave.sync_resync_count
        }
        Cmd::ReadTelemetry => {
            if let Some(&value) = slave.telemetry.get(parsed.payload[0] as usize) {
                value
            } else {
                slave.error_detail = ERR_INVALID_PAYLOAD;
                ERR_INVALID_PAYLOAD
            }
        }
        Cmd::WritePatternBuffer
        | Cmd::WritePatternCompressed
        | Cmd::WritePatternFused
        | Cmd::WriteModulationBuffer
        | Cmd::WriteModulationFused
        | Cmd::ConfigModulation
        | Cmd::ConfigPattern
        | Cmd::ChangePatternBank
        | Cmd::ChangeModulationBank
        | Cmd::SetSilencer
        | Cmd::SetPhaseCorrection
        | Cmd::SetPulseWidthTable
        | Cmd::EmulateGpioIn
        | Cmd::SetGpioOut
        | Cmd::ForceFan
        | Cmd::Synchronize
        | Cmd::Clear => 0,
        Cmd::SetOutputMask => {
            slave.muted = parsed.payload[..2] == [0, 0];
            0
        }
        Cmd::SetMode => {
            slave.mode = parsed.payload[0];
            0
        }
        Cmd::Reset | Cmd::Stop => unreachable!(),
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
async fn successful_send_advances_seq_and_leaves_no_error() {
    let (client, slave) = open_client().await;
    send_nop(&client).await.unwrap();

    let s = slave.lock().unwrap();
    assert_eq!(s.ack, 2);
    assert_eq!(s.expected_seq, 3);
    assert_eq!(s.error_detail, 0);
}

#[tokio::test]
async fn device_reported_error_becomes_device_error() {
    let (client, _slave) = open_client().await;
    let err = send_op(&client, FailingCmd).await.unwrap_err();
    match err {
        Error::DeviceError { device, code } => {
            assert_eq!(device, 0);
            assert_eq!(code, ERR_INVALID_DATA);
        }
        other => panic!("expected DeviceError, got {other:?}"),
    }
}

#[tokio::test]
async fn read_firmware_version_returns_full_triplet() {
    let (client, slave) = open_client().await;
    {
        let mut s = slave.lock().unwrap();
        s.fw_version_major = 1;
        s.fw_version_minor = 2;
        s.fw_version_patch = 3;
        s.fpga_version_major = 4;
        s.fpga_version_minor = 5;
        s.fpga_version_patch = 6;
    }
    let v = client.read_firmware_version().await.unwrap();
    assert_eq!(
        v,
        vec![FirmwareVersion {
            cpu: Version {
                major: 1,
                minor: 2,
                patch: 3,
            },
            fpga: Version {
                major: 4,
                minor: 5,
                patch: 6,
            },
            function_bits: 0,
        }]
    );
    assert!(!v[0].is_emulator());
    assert_eq!(v[0].to_string(), "CPU: 1.2.3, FPGA: 4.5.6");
}

#[tokio::test]
async fn read_firmware_version_reports_unknown_fpga_on_outdated_firmware() {
    let (client, slave) = open_client().await;
    {
        let mut s = slave.lock().unwrap();
        s.fw_version_major = 0;
        s.fw_version_minor = 1;
        s.fw_version_patch = 0;
        s.supports_fpga_version = false;
    }
    let v = client.read_firmware_version().await.unwrap();
    assert_eq!(
        v,
        vec![FirmwareVersion {
            cpu: Version {
                major: 0,
                minor: 1,
                patch: 0,
            },
            fpga: Version::UNKNOWN,
            function_bits: 0,
        }]
    );
    assert!(v[0].fpga.is_unknown());
    assert_eq!(v[0].to_string(), "CPU: 0.1.0, FPGA: unknown");
}

#[tokio::test]
async fn read_firmware_version_reports_unknown_fpga_when_error_detail_already_latched() {
    let (client, slave) = open_client().await;
    {
        let mut s = slave.lock().unwrap();
        s.supports_fpga_version = false;
        s.error_detail = ERR_UNKNOWN_CMD;
    }
    let v = client.read_firmware_version().await.unwrap();
    assert_eq!(v[0].fpga, Version::UNKNOWN);
}

#[tokio::test]
async fn read_firmware_version_keeps_fpga_version_when_unrelated_error_is_latched() {
    let (client, slave) = open_client().await;
    {
        let mut s = slave.lock().unwrap();
        s.fpga_version_major = 4;
        s.fpga_version_minor = 5;
        s.fpga_version_patch = 6;
        s.error_detail = ERR_INVALID_DATA;
    }
    let v = client.read_firmware_version().await.unwrap();
    assert_eq!(
        v[0].fpga,
        Version {
            major: 4,
            minor: 5,
            patch: 6,
        }
    );
}

#[tokio::test]
async fn read_firmware_version_reports_unknown_fpga_per_device() {
    let (link, slaves) = slaves_pair(2);
    {
        let mut s = slaves[0].lock().unwrap();
        s.fpga_version_major = 1;
        s.fpga_version_minor = 2;
        s.fpga_version_patch = 3;
    }
    slaves[1].lock().unwrap().supports_fpga_version = false;
    let client = Client::open(&geometry(2), link, ClientConfig::default())
        .await
        .unwrap();
    let v = client.read_firmware_version().await.unwrap();
    assert_eq!(
        v[0].fpga,
        Version {
            major: 1,
            minor: 2,
            patch: 3,
        }
    );
    assert_eq!(v[1].fpga, Version::UNKNOWN);
}

#[tokio::test]
async fn read_error_detail_returns_error_code() {
    let (client, slave) = open_client().await;
    slave.lock().unwrap().error_detail = 0x7A;
    let e = client.read_error_detail().await.unwrap();
    assert_eq!(e, vec![0x7A]);
}

#[tokio::test]
async fn device_error_is_observable_via_read_error_detail() {
    let (client, _slave) = open_client().await;
    let _ = send_op(&client, FailingCmd).await;
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
        s0.fpga_version_major = 0xA3;
        s0.fpga_version_minor = 0xA4;
        s0.fpga_version_patch = 0xA5;
        let mut s1 = slaves[1].lock().unwrap();
        s1.fw_version_major = 0xB0;
        s1.fw_version_minor = 0xB1;
        s1.fw_version_patch = 0xB2;
        s1.fpga_version_major = 0xB3;
        s1.fpga_version_minor = 0xB4;
        s1.fpga_version_patch = 0xB5;
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
                send_nop(&client).await.unwrap();
            }
        })
    };

    let expected = vec![
        FirmwareVersion {
            cpu: Version {
                major: 0xA0,
                minor: 0xA1,
                patch: 0xA2,
            },
            fpga: Version {
                major: 0xA3,
                minor: 0xA4,
                patch: 0xA5,
            },
            function_bits: 0,
        },
        FirmwareVersion {
            cpu: Version {
                major: 0xB0,
                minor: 0xB1,
                patch: 0xB2,
            },
            fpga: Version {
                major: 0xB3,
                minor: 0xB4,
                patch: 0xB5,
            },
            function_bits: 0,
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

    let ok = Datagram {
        cmd: Cmd::Nop,
        payload: [0u8; PAYLOAD_BYTES],
    };
    let bad_payload = failing_payload();

    let fut = client
        .send_datagrams(&[
            ok,
            Datagram {
                cmd: Cmd::Nop,
                payload: bad_payload,
            },
        ])
        .await
        .unwrap();
    let resp = fut.await.unwrap();
    assert_eq!(resp.data(), [0, ERR_INVALID_DATA]);
}

#[tokio::test]
async fn multi_device_send_reports_failing_device_index() {
    let (link, slaves) = slaves_pair(2);
    let client = Client::open(&geometry(2), link, ClientConfig::default())
        .await
        .unwrap();
    let err = send_op(&client, FailingCmd).await.unwrap_err();
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
            rt_policy: RtSchedulePolicy::default(),
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
            f.await.unwrap().data(),
            [0xB0, 0xB1],
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
        assert_eq!(
            s.mode,
            Mode::LowLatency.as_u8(),
            "slave must switch to low-latency"
        );
        assert!(s.sent_log.contains(&(0, Cmd::SetMode)));
        assert_eq!(s.expected_seq, 3);
    }
    send_nop(&client).await.unwrap();
    assert_eq!(slave.lock().unwrap().expected_seq, 4);
}

#[tokio::test]
async fn default_config_leaves_slave_in_fifo_mode() {
    let (_client, slave) = open_client().await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    let s = slave.lock().unwrap();
    assert_eq!(s.mode, Mode::Fifo.as_u8());
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
    send_nop(&client).await.unwrap();
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
    assert_eq!(r1.data(), [0xAA]);
    assert_eq!(r2.data(), [0xBB]);
}

#[tokio::test]
async fn pipeline_continues_after_device_error_in_the_middle() {
    let (client, slave) = open_client().await;
    slave.lock().unwrap().fw_version_major = 0x42;

    let bad_payload = failing_payload();

    let f1 = client
        .send_broadcast(&Datagram::no_payload(Cmd::ReadCpuFwVersionMajor))
        .await
        .unwrap();
    let f2 = client
        .send_broadcast(&Datagram {
            cmd: Cmd::Nop,
            payload: bad_payload,
        })
        .await
        .unwrap();
    let f3 = client
        .send_broadcast(&Datagram::no_payload(Cmd::ReadCpuFwVersionMajor))
        .await
        .unwrap();

    assert_eq!(f1.await.unwrap().data(), [0x42]);
    let mid = f2.await.unwrap();
    assert_eq!(mid.data(), [ERR_INVALID_DATA]);
    assert_eq!(f3.await.unwrap().data(), [0x42]);
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
            rt_policy: RtSchedulePolicy::default(),
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
            f.await.unwrap().data(),
            [0xAB],
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
            rt_policy: RtSchedulePolicy::default(),
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
    let err = send_nop(&client).await.unwrap_err();
    match err {
        Error::Timeout { cycles } => assert_eq!(cycles, 10),
        other => panic!("expected Timeout, got {other:?}"),
    }
}

#[tokio::test]
async fn recovers_after_transient_stale_cycles() {
    let (client, slave) = open_client().await;
    slave.lock().unwrap().stale_for_next = 3;
    send_nop(&client)
        .await
        .expect("send should recover after the stale burst");
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
        fut.await.unwrap().data(),
        [0xAB],
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
            rt_policy: RtSchedulePolicy::default(),
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
            f.await.unwrap().data(),
            [0xAB],
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
        fut.await.unwrap().data(),
        [0xCD],
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
            rt_policy: RtSchedulePolicy::default(),
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
        s.fpga_version_major = 0x44;
        s.fpga_version_minor = 0x55;
        s.fpga_version_patch = 0x66;
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
            rt_policy: RtSchedulePolicy::default(),
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
            cpu: Version {
                major: 0x11,
                minor: 0x22,
                patch: 0x33,
            },
            fpga: Version {
                major: 0x44,
                minor: 0x55,
                patch: 0x66,
            },
            function_bits: 0,
        }]
    );
}

#[tokio::test]
async fn build_rejects_too_fast_pattern_under_strict_silencer() {
    use crate::commands::operation::{ConfigPattern, SetSilencer};
    use crate::value::{LoopBehavior, PatternBank, SamplingConfig};

    let (client, _slave) = open_client().await;
    let mut builder = client.datagram_builder();
    builder.push(SetSilencer::default()).push(ConfigPattern {
        bank: PatternBank::B0,
        config: SamplingConfig::FREQ_40K,
        size: 2,
        loop_behavior: LoopBehavior::Infinite,
    });
    match builder.build().unwrap_err() {
        Error::SilencerConstraint {
            device,
            axis,
            completion_steps,
            sampling_div,
        } => {
            assert_eq!(device, 0);
            assert_eq!(axis, crate::mirror::SilencerAxis::Intensity);
            assert_eq!(completion_steps, 10);
            assert_eq!(sampling_div, 1);
        }
        other => panic!("expected SilencerConstraint, got {other:?}"),
    }
}

#[tokio::test]
async fn build_rejects_strict_silencer_when_active_sampling_too_fast() {
    use crate::commands::operation::{ConfigModulation, FixedCompletionTime, SetSilencer};
    use crate::common::ULTRASOUND_PERIOD;
    use crate::value::{LoopBehavior, ModulationBank, SamplingConfig};
    use core::num::NonZeroU16;

    let (client, _slave) = open_client().await;
    let mut builder = client.datagram_builder();
    builder
        .push(ConfigModulation {
            bank: ModulationBank::B0,
            config: SamplingConfig::new(NonZeroU16::new(5).unwrap()),
            size: 2,
            loop_behavior: LoopBehavior::Infinite,
        })
        .push(SetSilencer::new(FixedCompletionTime {
            intensity: ULTRASOUND_PERIOD * 8,
            phase: ULTRASOUND_PERIOD * 40,
            strict_mode: true,
        }));
    assert!(matches!(
        builder.build().unwrap_err(),
        Error::SilencerConstraint {
            axis: crate::mirror::SilencerAxis::Intensity,
            completion_steps: 8,
            sampling_div: 5,
            ..
        }
    ));
}

#[tokio::test]
async fn opt_out_disables_precheck() {
    use crate::commands::operation::{ConfigPattern, SetSilencer};
    use crate::value::{LoopBehavior, PatternBank, SamplingConfig};

    let (link, _slave) = slave_pair();
    let config = ClientConfig {
        validate_state: false,
        ..ClientConfig::default()
    };
    let client = Client::open(&geometry(1), link, config).await.unwrap();
    let mut builder = client.datagram_builder();
    builder.push(SetSilencer::default()).push(ConfigPattern {
        bank: PatternBank::B0,
        config: SamplingConfig::FREQ_40K,
        size: 2,
        loop_behavior: LoopBehavior::Infinite,
    });
    assert!(
        builder.build().is_ok(),
        "opt-out must skip the local pre-check and defer to the CPU guard"
    );
}

#[tokio::test]
async fn desync_after_send_failure_stops_precheck() {
    use crate::commands::operation::{ConfigPattern, SetSilencer};
    use crate::value::{LoopBehavior, PatternBank, SamplingConfig};

    let too_fast = |client: &Client| {
        let mut builder = client.datagram_builder();
        builder.push(ConfigPattern {
            bank: PatternBank::B0,
            config: SamplingConfig::FREQ_40K,
            size: 2,
            loop_behavior: LoopBehavior::Infinite,
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
        send_nop(&client).await.unwrap_err(),
        Error::Timeout { .. }
    ));

    assert!(
        too_fast(&client).is_ok(),
        "desynced mirror must stop pre-checking until the next Clear/reopen"
    );
}

#[tokio::test]
async fn build_rejects_transition_mode_incompatible_with_loop() {
    use crate::commands::Modulation;
    use crate::value::{LoopBehavior, SamplingConfig, TransitionMode};
    use core::num::NonZeroU16;

    let data = [0x80u8; 4];
    let finite = LoopBehavior::Finite(NonZeroU16::new(2).unwrap());

    let (client, _slave) = open_client().await;

    let mut builder = client.datagram_builder();
    builder.push(Modulation {
        loop_behavior: finite,
        transition_mode: TransitionMode::Immediate,
        ..Modulation::new(SamplingConfig::FREQ_4K, &data)
    });
    assert!(
        matches!(
            builder.build().unwrap_err(),
            Error::TransitionConstraint {
                device: 0,
                transition_mode: TransitionMode::Immediate,
                bank_loop: crate::mirror::BankLoop::Finite,
            }
        ),
        "finite loop must reject an immediate transition"
    );

    let mut builder = client.datagram_builder();
    builder.push(Modulation {
        loop_behavior: finite,
        transition_mode: TransitionMode::SyncIdx,
        ..Modulation::new(SamplingConfig::FREQ_4K, &data)
    });
    assert!(
        builder.build().is_ok(),
        "finite loop with a timed transition is valid"
    );
}

#[tokio::test]
async fn build_rejects_timed_transition_on_infinite_loop() {
    use crate::commands::stm::{FociStm, FociStmOption};
    use crate::geometry::Point3;
    use crate::value::{ControlPoints, GpioIn, LoopBehavior, SamplingConfig, TransitionMode};

    let points = [
        ControlPoints::from(Point3::new(0.0, 0.0, 150.0)),
        ControlPoints::from(Point3::new(0.0, 0.0, 200.0)),
    ];

    let (client, _slave) = open_client().await;

    let mut builder = client.datagram_builder();
    builder.push(FociStm::new(
        SamplingConfig::FREQ_4K,
        &points,
        FociStmOption {
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::Gpio(GpioIn::I1),
            ..Default::default()
        },
    ));
    assert!(
        matches!(
            builder.build().unwrap_err(),
            Error::TransitionConstraint {
                bank_loop: crate::mirror::BankLoop::Infinite,
                ..
            }
        ),
        "infinite loop must reject a GPIO-timed transition"
    );

    let mut builder = client.datagram_builder();
    builder.push(FociStm::new(
        SamplingConfig::FREQ_4K,
        &points,
        FociStmOption::default(),
    ));
    assert!(
        builder.build().is_ok(),
        "infinite loop with the default immediate transition is valid"
    );
}

#[tokio::test]
async fn transition_precheck_opts_out_with_validate_state() {
    use crate::commands::Modulation;
    use crate::value::{LoopBehavior, SamplingConfig, TransitionMode};
    use core::num::NonZeroU16;

    let (link, _slave) = slave_pair();
    let config = ClientConfig {
        validate_state: false,
        ..ClientConfig::default()
    };
    let client = Client::open(&geometry(1), link, config).await.unwrap();

    let data = [0x80u8; 4];
    let mut builder = client.datagram_builder();
    builder.push(Modulation {
        loop_behavior: LoopBehavior::Finite(NonZeroU16::new(2).unwrap()),
        transition_mode: TransitionMode::Immediate,
        ..Modulation::new(SamplingConfig::FREQ_4K, &data)
    });
    assert!(
        builder.build().is_ok(),
        "opt-out must skip the transition pre-check and defer to the firmware"
    );
}

#[tokio::test]
async fn build_rejects_per_device_group_under_strict_silencer() {
    use crate::commands::operation::{ConfigModulation, SetSilencer};
    use crate::value::{LoopBehavior, ModulationBank, SamplingConfig};
    use core::num::NonZeroU16;

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
            config: SamplingConfig::new(
                NonZeroU16::new(if device.idx() == 0 { 5 } else { 20 }).unwrap(),
            ),
            size: 2,
            loop_behavior: LoopBehavior::Infinite,
        })
    });
    match builder.build().unwrap_err() {
        Error::SilencerConstraint { device, .. } => assert_eq!(device, 0),
        other => panic!("expected SilencerConstraint on device 0, got {other:?}"),
    }
}

#[tokio::test]
async fn separate_builders_share_committed_mirror_state() {
    use crate::commands::operation::{ConfigPattern, SetSilencer};
    use crate::value::{LoopBehavior, PatternBank, SamplingConfig};

    let (client, _slave) = open_client().await;
    client
        .datagram_builder()
        .push(SetSilencer::default())
        .build()
        .unwrap();
    let mut b2 = client.datagram_builder();
    b2.push(ConfigPattern {
        bank: PatternBank::B0,
        config: SamplingConfig::FREQ_40K,
        size: 2,
        loop_behavior: LoopBehavior::Infinite,
    });
    assert!(matches!(b2.build(), Err(Error::SilencerConstraint { .. })));
}

#[tokio::test]
async fn stop_mutes_via_the_stop_command() {
    let (client, slave) = open_client().await;

    client.stop().await.unwrap();

    let s = slave.lock().unwrap();
    assert!(s.muted);
    assert!(s.sent_log.iter().any(|(_, cmd)| *cmd == Cmd::Stop));
}

#[tokio::test]
async fn stop_resyncs_seq_and_lets_later_frames_through() {
    let (client, slave) = open_client().await;

    client.stop().await.unwrap();
    client.read_error_detail().await.unwrap();

    let s = slave.lock().unwrap();
    assert_eq!(s.ack, s.expected_seq.wrapping_sub(1));
}

#[tokio::test]
async fn read_telemetry_returns_selected_counter() {
    let (client, slave) = open_client().await;
    slave.lock().unwrap().telemetry[Telemetry::FifoDrop.as_u8() as usize] = 7;
    slave.lock().unwrap().telemetry[Telemetry::Failsafe.as_u8() as usize] = 3;

    assert_eq!(
        client.read_telemetry(Telemetry::FifoDrop).await.unwrap(),
        vec![7]
    );
    assert_eq!(
        client.read_telemetry(Telemetry::Failsafe).await.unwrap(),
        vec![3]
    );
}

#[tokio::test]
async fn read_telemetry_returns_sync_resync_count() {
    let (client, slave) = open_client().await;
    slave.lock().unwrap().sync_resync_count = 5;

    assert_eq!(
        client.read_telemetry(Telemetry::SyncResync).await.unwrap(),
        vec![5]
    );
}

#[tokio::test]
async fn read_firmware_version_reports_emulator_bit() {
    let (client, slave) = open_client().await;
    {
        let mut s = slave.lock().unwrap();
        s.fpga_version_major = 4;
        s.fpga_version_minor = 5;
        s.fpga_version_patch = 6;
        s.fpga_functions = 1 << 7;
    }

    let v = client.read_firmware_version().await.unwrap();
    assert!(v[0].is_emulator());
    assert_eq!(v[0].to_string(), "CPU: 0.0.0, FPGA: 4.5.6 [Emulator]");
}

#[tokio::test]
async fn read_firmware_version_without_emulator_bit_is_not_emulator() {
    let (client, slave) = open_client().await;
    slave.lock().unwrap().fpga_functions = 0x7F;

    let v = client.read_firmware_version().await.unwrap();
    assert!(!v[0].is_emulator());
    assert!(!v[0].to_string().contains("[Emulator]"));
}

#[tokio::test]
async fn read_firmware_version_ignores_emulator_bit_when_fpga_unknown() {
    let (client, slave) = open_client().await;
    {
        let mut s = slave.lock().unwrap();
        s.supports_fpga_version = false;
        s.fpga_functions = 1 << 7;
    }

    let v = client.read_firmware_version().await.unwrap();
    assert!(!v[0].is_emulator());
}
