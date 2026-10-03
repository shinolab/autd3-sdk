use std::collections::VecDeque;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use std::time::{Duration, Instant};

use crate::commands::operation::{
    Distribution, Encoded, Nop, Operation, PATTERN_FUSED_HEADER_BYTES,
};
use crate::datagram::Datagram;
use crate::error::Error;
use crate::firmware_version::{FirmwareVersion, Version};
use crate::geometry::Device;
use crate::geometry::{Autd3, Geometry};
use crate::protocol::{Cmd, MAX_INFLIGHT, PAYLOAD_BYTES, TxFrame};
use crate::response::Response;
use crate::transport::{Bus, FrameBuf};
use crate::udp::Reply;
use autd3_rs_core::BusStats;

use crate::telemetry::Telemetry;
use autd3_cpu_wire::Mode;

use super::{Client, ClientConfig};

fn first_bytes(response: &Response) -> Vec<u8> {
    response
        .values()
        .iter()
        .map(|value| value.first().copied().unwrap_or(0))
        .collect()
}

fn geometry(n: usize) -> Geometry {
    Geometry::new((0..n).map(|_| Autd3::default()).collect())
}

const FAIL_MARKER: u8 = 0xAA;

struct FailingCmd;

impl crate::sealed::Sealed for FailingCmd {}

impl Operation for FailingCmd {
    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(&self, _device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        out[0] = FAIL_MARKER;
        Ok(Encoded::new(Cmd::Nop, 1))
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

fn build_too_fast_pattern(client: &Client) -> Result<crate::datagram::Frames, Error> {
    use crate::commands::operation::ConfigPattern;
    use crate::value::{LoopBehavior, PatternBank, SamplingConfig};

    let mut builder = client.datagram_builder();
    builder.push(ConfigPattern {
        bank: PatternBank::B0,
        config: SamplingConfig::FREQ_40K,
        size: 2,
        loop_behavior: LoopBehavior::Infinite,
    });
    builder.build()
}

async fn arm_strict_silencer(client: &Client) {
    use crate::commands::operation::SetSilencer;

    let datagrams = client
        .datagram_builder()
        .push(SetSilencer::default())
        .build()
        .unwrap();
    for frame in &datagrams {
        client.send_checked(frame).await.unwrap();
    }
}

struct LoopbackLink {
    slaves: Vec<Arc<StdMutex<Slave>>>,
    queue: VecDeque<Reply>,
    msg_id: u16,
}

struct Slave {
    expected_seq: u8,
    ack: u8,
    status: u8,
    value: Vec<u8>,
    fw_version_major: u8,
    fw_version_minor: u8,
    fw_version_patch: u8,
    fpga_version_major: u8,
    fpga_version_minor: u8,
    fpga_version_patch: u8,
    error_detail: u8,
    fpga_state: u8,
    fpga_functions: u8,
    telemetry: [u32; Telemetry::ALL.len()],
    short_reads: bool,
    muted: bool,
    drop_next: u32,
    silent_until: Option<Instant>,
    sent_log: Vec<(u8, Cmd)>,
    mode: u8,
}

impl Slave {
    fn new() -> Self {
        Self {
            expected_seq: 0,
            ack: 0xFF,
            status: 0,
            value: Vec::new(),
            fw_version_major: 0,
            fw_version_minor: 0,
            fw_version_patch: 0,
            fpga_version_major: 0,
            fpga_version_minor: 0,
            fpga_version_patch: 0,
            error_detail: 0,
            fpga_state: 0,
            fpga_functions: 0,
            telemetry: [0; Telemetry::ALL.len()],
            short_reads: false,
            muted: false,
            drop_next: 0,
            silent_until: None,
            sent_log: Vec::new(),
            mode: Mode::Fifo.as_u8(),
        }
    }

    fn silence(&mut self, duration: Duration) {
        self.silent_until = Some(Instant::now() + duration);
    }

    fn silence_forever(&mut self) {
        self.silence(Duration::from_secs(3600));
    }

    fn silent(&self) -> bool {
        self.silent_until
            .is_some_and(|until| Instant::now() < until)
    }

    fn reply(&self, device: usize, msg_id: u16) -> Reply {
        Reply::new(device, msg_id, self.ack, self.status, 0x09, &self.value)
    }
}

const ERR_UNKNOWN_CMD: u8 = 0x01;
const ERR_INVALID_DATA: u8 = 0x03;

fn handle_nop(payload: &[u8; PAYLOAD_BYTES], slave: &mut Slave) -> u8 {
    if payload[0] == FAIL_MARKER {
        slave.error_detail = ERR_INVALID_DATA;
        ERR_INVALID_DATA
    } else {
        0
    }
}

fn read_value(slave: &Slave, cmd: Cmd) -> Vec<u8> {
    let value = match cmd {
        Cmd::ReadFirmwareInfo => vec![
            slave.fw_version_major,
            slave.fw_version_minor,
            slave.fw_version_patch,
            slave.fpga_version_major,
            slave.fpga_version_minor,
            slave.fpga_version_patch,
            slave.fpga_functions,
            0,
        ],
        Cmd::ReadErrorDetail => vec![slave.error_detail],
        Cmd::ReadFpgaState => vec![slave.fpga_state],
        Cmd::ReadTelemetry => slave
            .telemetry
            .iter()
            .flat_map(|c| c.to_le_bytes())
            .collect(),
        _ => Vec::new(),
    };
    if slave.short_reads {
        value[..value.len() / 2].to_vec()
    } else {
        value
    }
}

fn slave_frame(slave: &mut Slave, frame: &[u8]) -> bool {
    let parsed = TxFrame::parse(frame).expect("loopback only sees known cmds");
    slave.sent_log.push((parsed.seq.get(), parsed.cmd));

    if parsed.cmd == Cmd::Reset {
        slave.expected_seq = 0;
        slave.ack = 0xFF;
        slave.status = 0;
        slave.value.clear();
        return true;
    }

    if slave.silent() {
        return false;
    }

    if parsed.seq.get() != slave.expected_seq {
        return true;
    }

    if slave.drop_next > 0 {
        slave.drop_next -= 1;
        return false;
    }

    slave.expected_seq = slave.expected_seq.wrapping_add(1);
    slave.value.clear();
    let status = match parsed.cmd {
        Cmd::Nop => handle_nop(&parsed.payload, slave),
        Cmd::ReadFirmwareInfo | Cmd::ReadErrorDetail | Cmd::ReadFpgaState | Cmd::ReadTelemetry => {
            slave.value = read_value(slave, parsed.cmd);
            0
        }
        Cmd::WritePatternFused => {
            let start = PATTERN_FUSED_HEADER_BYTES + Autd3::NUM_TRANSDUCERS;
            let end = start + Autd3::NUM_TRANSDUCERS;
            slave.muted = parsed.payload[start..end].iter().all(|&b| b == 0);
            0
        }
        Cmd::WritePatternRaw
        | Cmd::WriteFociBuffer
        | Cmd::WritePatternCompressed
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
        _ => ERR_UNKNOWN_CMD,
    };
    slave.ack = parsed.seq.get();
    slave.status = status;
    true
}

impl LoopbackLink {
    fn wait(deadline: Instant) {
        let now = Instant::now();
        if deadline > now {
            std::thread::sleep(deadline - now);
        }
    }
}

impl Bus for LoopbackLink {
    type Error = std::convert::Infallible;

    fn num_devices(&self) -> usize {
        self.slaves.len()
    }

    fn next_msg_id(&self) -> u16 {
        self.msg_id.wrapping_add(1)
    }

    fn send(&mut self, frames: &[FrameBuf]) -> Result<u16, Self::Error> {
        self.msg_id = self.msg_id.wrapping_add(1);
        for (device, (frame, slave)) in frames.iter().zip(&self.slaves).enumerate() {
            let mut s = slave.lock().unwrap();
            if slave_frame(&mut s, frame.as_ref()) {
                self.queue.push_back(s.reply(device, self.msg_id));
            }
        }
        Ok(self.msg_id)
    }

    fn heartbeat(&mut self) -> Result<u16, Self::Error> {
        self.msg_id = self.msg_id.wrapping_add(1);
        for (device, slave) in self.slaves.iter().enumerate() {
            let s = slave.lock().unwrap();
            if !s.silent() {
                self.queue.push_back(s.reply(device, self.msg_id));
            }
        }
        Ok(self.msg_id)
    }

    fn try_recv(&mut self) -> Result<Option<Reply>, Self::Error> {
        Ok(self.queue.pop_front())
    }

    fn wait_readable(&mut self, deadline: Instant) -> Result<bool, Self::Error> {
        if !self.queue.is_empty() {
            return Ok(true);
        }
        Self::wait(deadline);
        Ok(false)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("link failure")]
struct LinkFailure;

fn link_cause_is<E: core::error::Error + Send + Sync + 'static>(e: &Error) -> bool {
    let Error::Network(cause) = e else {
        return false;
    };
    cause.downcast_ref::<E>().is_some()
        && core::error::Error::source(e).is_some_and(|s| s.downcast_ref::<E>().is_some())
}

struct FailingLink {
    inner: LoopbackLink,
    fail: Arc<AtomicBool>,
    slow_drop: Option<Arc<AtomicBool>>,
}

impl Drop for FailingLink {
    fn drop(&mut self) {
        if let Some(entered) = &self.slow_drop {
            entered.store(true, AtomicOrdering::Release);
            std::thread::sleep(Duration::from_millis(500));
        }
    }
}

impl Bus for FailingLink {
    type Error = LinkFailure;

    fn num_devices(&self) -> usize {
        self.inner.num_devices()
    }

    fn next_msg_id(&self) -> u16 {
        self.inner.next_msg_id()
    }

    fn send(&mut self, frames: &[FrameBuf]) -> Result<u16, Self::Error> {
        if self.fail.load(AtomicOrdering::Relaxed) {
            return Err(LinkFailure);
        }
        Ok(self.inner.send(frames).expect("loopback never fails"))
    }

    fn heartbeat(&mut self) -> Result<u16, Self::Error> {
        if self.fail.load(AtomicOrdering::Relaxed) {
            return Err(LinkFailure);
        }
        Ok(self.inner.heartbeat().expect("loopback never fails"))
    }

    fn try_recv(&mut self) -> Result<Option<Reply>, Self::Error> {
        if self.fail.load(AtomicOrdering::Relaxed) {
            return Err(LinkFailure);
        }
        Ok(self.inner.try_recv().expect("loopback never fails"))
    }

    fn wait_readable(&mut self, deadline: Instant) -> Result<bool, Self::Error> {
        Ok(self
            .inner
            .wait_readable(deadline)
            .expect("loopback never fails"))
    }
}

fn slaves_pair(n: usize) -> (LoopbackLink, Vec<Arc<StdMutex<Slave>>>) {
    let slaves: Vec<_> = (0..n)
        .map(|_| Arc::new(StdMutex::new(Slave::new())))
        .collect();
    (
        LoopbackLink {
            slaves: slaves.clone(),
            queue: VecDeque::new(),
            msg_id: 0,
        },
        slaves,
    )
}

fn slave_pair() -> (LoopbackLink, Arc<StdMutex<Slave>>) {
    let (link, mut slaves) = slaves_pair(1);
    (link, slaves.pop().expect("one slave"))
}

fn seq_after_open(slave: &Arc<StdMutex<Slave>>) -> u8 {
    slave.lock().unwrap().expected_seq
}

async fn open_client() -> (Client, Arc<StdMutex<Slave>>) {
    let (link, slave) = slave_pair();
    let client = Client::open_bus(&geometry(1), link, ClientConfig::default())
        .await
        .unwrap();
    (client, slave)
}

#[tokio::test]
async fn successful_send_advances_seq_and_leaves_no_error() {
    let (client, slave) = open_client().await;
    let base = seq_after_open(&slave);
    send_nop(&client).await.unwrap();

    let s = slave.lock().unwrap();
    assert_eq!(s.ack, base);
    assert_eq!(s.expected_seq, base + 1);
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

fn set_supported_series(slave: &Arc<StdMutex<Slave>>) {
    let (major, minor) = FirmwareVersion::SUPPORTED_SERIES;
    let mut s = slave.lock().unwrap();
    s.fw_version_major = major;
    s.fw_version_minor = minor;
    s.fpga_version_major = major;
    s.fpga_version_minor = minor;
}

#[tokio::test]
async fn the_bundled_series_is_reported_as_supported() {
    let (client, slave) = open_client().await;
    set_supported_series(&slave);

    let v = client.read_firmware_version().await.unwrap();
    assert!(v[0].is_supported());
}

#[tokio::test]
async fn a_foreign_series_is_reported_as_unsupported() {
    let (client, slave) = open_client().await;
    set_supported_series(&slave);
    slave.lock().unwrap().fw_version_minor = FirmwareVersion::SUPPORTED_SERIES.1.wrapping_add(1);

    let v = client.read_firmware_version().await.unwrap();
    assert!(!v[0].is_supported());
}

#[tokio::test]
async fn an_unknown_fpga_version_is_never_supported() {
    let (client, slave) = open_client().await;
    set_supported_series(&slave);
    {
        let mut s = slave.lock().unwrap();
        s.fpga_version_major = 0;
        s.fpga_version_minor = 0;
        s.fpga_version_patch = 0;
    }

    let v = client.read_firmware_version().await.unwrap();
    assert!(v[0].fpga.is_unknown());
    assert!(!v[0].is_supported());
}

#[tokio::test]
async fn open_rejects_a_foreign_series_only_when_the_check_is_requested() {
    let config = ClientConfig {
        require_supported_firmware: true,
        ..Default::default()
    };

    let (link, slave) = slave_pair();
    set_supported_series(&slave);
    let client = Client::open_bus(&geometry(1), link, config).await.unwrap();
    client.close().await.unwrap();

    let (link, slave) = slave_pair();
    set_supported_series(&slave);
    slave.lock().unwrap().fpga_version_major = FirmwareVersion::SUPPORTED_SERIES.0.wrapping_add(1);
    let opened = Client::open_bus(&geometry(1), link, config).await;
    assert!(matches!(
        opened.err(),
        Some(Error::UnsupportedFirmware { device: 0, .. })
    ));

    let (link, slave) = slave_pair();
    set_supported_series(&slave);
    slave.lock().unwrap().fpga_version_major = FirmwareVersion::SUPPORTED_SERIES.0.wrapping_add(1);
    Client::open_bus(&geometry(1), link, ClientConfig::default())
        .await
        .unwrap()
        .close()
        .await
        .unwrap();
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
        Client::open_bus(&geometry(2), link, ClientConfig::default())
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
    let client = Client::open_bus(&geometry(2), link, ClientConfig::default())
        .await
        .unwrap();

    let ok = Datagram::no_payload(Cmd::Nop);
    let bad_payload = failing_payload();

    let fut = client
        .send_datagrams(&[
            ok,
            Datagram {
                cmd: Cmd::Nop,
                payload: bad_payload,
                payload_len: 1,
            },
        ])
        .await
        .unwrap();
    let resp = fut.await.unwrap();
    assert_eq!(resp.status(), [0, ERR_INVALID_DATA]);
}

#[tokio::test]
async fn multi_device_send_reports_failing_device_index() {
    let (link, slaves) = slaves_pair(2);
    let client = Client::open_bus(&geometry(2), link, ClientConfig::default())
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
    slaves[1].lock().unwrap().fpga_state = 0xB1;
    slaves[0].lock().unwrap().fpga_state = 0xB0;
    let client = Client::open_bus(
        &geometry(2),
        link,
        ClientConfig {
            ack_timeout: Duration::from_millis(10),
            max_inflight: NonZeroUsize::new(16).unwrap(),
            max_resync_rounds: NonZeroU32::new(8).unwrap(),
            low_latency: false,
            validate_state: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let base = seq_after_open(&slaves[0]);
    slaves[1].lock().unwrap().drop_next = 1;

    let mut futs = Vec::new();
    for _ in 0..8 {
        futs.push(
            client
                .send_broadcast(&Datagram::no_payload(Cmd::ReadFpgaState))
                .await
                .unwrap(),
        );
    }
    for f in futs {
        assert_eq!(
            first_bytes(&f.await.unwrap()),
            [0xB0, 0xB1],
            "resync must recover as success with per-device data"
        );
    }
    assert_eq!(slaves[0].lock().unwrap().expected_seq, base + 8);
    assert_eq!(slaves[1].lock().unwrap().expected_seq, base + 8);
}

#[tokio::test]
async fn send_rejects_wrong_datagram_count() {
    let (link, _slaves) = slaves_pair(2);
    let client = Client::open_bus(&geometry(2), link, ClientConfig::default())
        .await
        .unwrap();
    let err = client
        .send_datagrams(&[Datagram::no_payload(Cmd::ReadFpgaState)])
        .await
        .err()
        .expect("send with wrong datagram count must fail");
    assert!(matches!(err, Error::InvalidPayload(_)));
}

#[tokio::test]
async fn handshake_confirms_one_reset_then_sets_the_mode() {
    let (_client, slave) = open_client().await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    let s = slave.lock().unwrap();
    assert!(s.sent_log.len() >= 2);
    assert_eq!(s.sent_log[0], (0, Cmd::Reset));
    assert_eq!(s.sent_log[1], (0, Cmd::SetMode));
    let clear = s.sent_log.iter().position(|(_, c)| *c == Cmd::Clear);
    let synchronize = s.sent_log.iter().position(|(_, c)| *c == Cmd::Synchronize);
    assert!(clear.is_some() && synchronize < Some(s.sent_log.len()));
    assert!(clear < synchronize, "{:?}", s.sent_log);
}

#[derive(Clone)]
struct CapturedLog(Arc<StdMutex<Vec<u8>>>);

impl std::io::Write for CapturedLog {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedLog {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

impl CapturedLog {
    fn mark(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    fn since(&self, mark: usize) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()[mark..]).into_owned()
    }
}

fn captured_warnings() -> &'static CapturedLog {
    static LOG: std::sync::OnceLock<CapturedLog> = std::sync::OnceLock::new();
    LOG.get_or_init(|| {
        let log = CapturedLog(Arc::new(StdMutex::new(Vec::new())));
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_writer(log.clone())
                .with_max_level(tracing::Level::WARN)
                .with_ansi(false)
                .finish(),
        );
        log
    })
}

#[tokio::test]
async fn a_foreign_series_warns_at_open_without_refusing_it() {
    let log = captured_warnings();

    let (link, slave) = slave_pair();
    set_supported_series(&slave);
    {
        let mut s = slave.lock().unwrap();
        s.fw_version_minor = FirmwareVersion::SUPPORTED_SERIES.1.wrapping_add(1);
        s.fw_version_patch = 0x5A;
        s.fpga_version_patch = 0x5B;
    }

    let mark = log.mark();
    let client = Client::open_bus(&geometry(1), link, ClientConfig::default())
        .await
        .expect("a foreign series must not refuse the default open");
    let captured = log.since(mark);
    client.close().await.unwrap();

    let (major, minor) = FirmwareVersion::SUPPORTED_SERIES;
    let version = format!(
        "CPU: {major}.{}.90, FPGA: {major}.{minor}.91",
        minor.wrapping_add(1),
    );
    assert!(
        captured.contains("outside the series supported by this SDK")
            && captured.contains(&version),
        "open did not warn about the series mismatch of {version}: {captured}",
    );
}

#[tokio::test]
async fn open_reads_the_firmware_version_even_when_the_check_is_off() {
    let (link, slave) = slave_pair();
    set_supported_series(&slave);
    slave.lock().unwrap().fw_version_minor = FirmwareVersion::SUPPORTED_SERIES.1.wrapping_add(1);

    let client = Client::open_bus(&geometry(1), link, ClientConfig::default())
        .await
        .unwrap();

    {
        let s = slave.lock().unwrap();
        assert!(
            s.sent_log.iter().any(|(_, c)| *c == Cmd::ReadFirmwareInfo),
            "ReadFirmwareInfo never reached the device, so the series mismatch cannot be warned about",
        );
    }
    client.close().await.unwrap();
}

#[tokio::test]
async fn low_latency_handshake_switches_slave_mode_and_continues_traffic() {
    let (link, slave) = slave_pair();
    let config = ClientConfig {
        low_latency: true,
        ..ClientConfig::default()
    };
    let client = Client::open_bus(&geometry(1), link, config).await.unwrap();
    {
        let s = slave.lock().unwrap();
        assert_eq!(
            s.mode,
            Mode::LowLatency.as_u8(),
            "slave must switch to low-latency"
        );
        assert!(s.sent_log.contains(&(0, Cmd::SetMode)));
    }
    let base = seq_after_open(&slave);
    send_nop(&client).await.unwrap();
    assert_eq!(slave.lock().unwrap().expected_seq, base + 1);
}

#[tokio::test]
async fn default_config_negotiates_fifo_mode() {
    let (_client, slave) = open_client().await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    let s = slave.lock().unwrap();
    assert_eq!(s.mode, Mode::Fifo.as_u8());
    assert!(s.sent_log.contains(&(0, Cmd::SetMode)));
}

#[tokio::test]
async fn handshake_clears_low_latency_left_by_a_previous_session() {
    let (link, slave) = slave_pair();
    slave.lock().unwrap().mode = Mode::LowLatency.as_u8();
    let client = Client::open_bus(&geometry(1), link, ClientConfig::default())
        .await
        .unwrap();
    {
        let s = slave.lock().unwrap();
        assert_eq!(
            s.mode,
            Mode::Fifo.as_u8(),
            "a low-latency slave must fall back to FIFO without a power cycle"
        );
    }
    send_nop(&client).await.unwrap();
    assert_eq!(slave.lock().unwrap().mode, Mode::Fifo.as_u8());
}

#[tokio::test]
async fn handshake_resets_slave_proto_state() {
    let (link, slave) = slave_pair();
    {
        let mut s = slave.lock().unwrap();
        s.expected_seq = 42;
        s.ack = 41;
    }
    let client = Client::open_bus(&geometry(1), link, ClientConfig::default())
        .await
        .unwrap();
    let base = {
        let s = slave.lock().unwrap();
        assert!(
            s.expected_seq < 42,
            "the handshake must restart the sequence, not continue the stale one",
        );
        assert_eq!(s.ack, s.expected_seq - 1);
        s.expected_seq
    };
    send_nop(&client).await.unwrap();
    assert_eq!(slave.lock().unwrap().expected_seq, base + 1);
}

#[tokio::test]
async fn two_stage_await_resolves_in_order() {
    let (client, slave) = open_client().await;
    {
        let mut s = slave.lock().unwrap();
        s.error_detail = 0xAA;
        s.fpga_state = 0xBB;
    }
    let f1 = client
        .send_broadcast(&Datagram::no_payload(Cmd::ReadErrorDetail))
        .await
        .unwrap();
    let f2 = client
        .send_broadcast(&Datagram::no_payload(Cmd::ReadFpgaState))
        .await
        .unwrap();
    let r1 = f1.await.unwrap();
    let r2 = f2.await.unwrap();
    assert_eq!(first_bytes(&r1), [0xAA]);
    assert_eq!(first_bytes(&r2), [0xBB]);
}

#[tokio::test]
async fn pipeline_continues_after_device_error_in_the_middle() {
    let (client, slave) = open_client().await;
    slave.lock().unwrap().fpga_state = 0x42;

    let bad_payload = failing_payload();

    let f1 = client
        .send_broadcast(&Datagram::no_payload(Cmd::ReadFpgaState))
        .await
        .unwrap();
    let f2 = client
        .send_broadcast(&Datagram {
            cmd: Cmd::Nop,
            payload: bad_payload,
            payload_len: 1,
        })
        .await
        .unwrap();
    let f3 = client
        .send_broadcast(&Datagram::no_payload(Cmd::ReadFpgaState))
        .await
        .unwrap();

    assert_eq!(first_bytes(&f1.await.unwrap()), [0x42]);
    let mid = f2.await.unwrap();
    assert_eq!(mid.status(), [ERR_INVALID_DATA]);
    assert_eq!(first_bytes(&f3.await.unwrap()), [0x42]);
}

#[tokio::test]
async fn streaming_skip_recovers_via_resync_without_timeout() {
    let (link, slave) = slave_pair();
    slave.lock().unwrap().fpga_state = 0xAB;
    let client = Client::open_bus(
        &geometry(1),
        link,
        ClientConfig {
            ack_timeout: Duration::from_millis(10),
            max_inflight: NonZeroUsize::new(16).unwrap(),
            max_resync_rounds: NonZeroU32::new(8).unwrap(),
            low_latency: false,
            validate_state: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let base = seq_after_open(&slave);
    slave.lock().unwrap().drop_next = 1;

    let mut futs = Vec::new();
    for _ in 0..8 {
        futs.push(
            client
                .send_broadcast(&Datagram::no_payload(Cmd::ReadFpgaState))
                .await
                .unwrap(),
        );
    }
    for f in futs {
        assert_eq!(
            first_bytes(&f.await.unwrap()),
            [0xAB],
            "resync must recover as success"
        );
    }
    assert_eq!(slave.lock().unwrap().expected_seq, base + 8);
}

#[tokio::test]
async fn dead_link_gives_up_whole_window_in_bounded_time() {
    let (link, slave) = slave_pair();
    let client = Client::open_bus(
        &geometry(1),
        link,
        ClientConfig {
            ack_timeout: Duration::from_millis(5),
            max_inflight: NonZeroUsize::new(8).unwrap(),
            max_resync_rounds: NonZeroU32::new(3).unwrap(),
            low_latency: false,
            validate_state: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    slave.lock().unwrap().drop_next = u32::MAX;

    let mut futs = Vec::new();
    for _ in 0..3 {
        futs.push(
            client
                .send_broadcast(&Datagram::no_payload(Cmd::ReadFpgaState))
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
        s.silence_forever();
    }
    let err = send_nop(&client).await.unwrap_err();
    match err {
        Error::Timeout { timeout } => assert_eq!(timeout, Duration::from_millis(10)),
        other => panic!("expected Timeout, got {other:?}"),
    }
}

#[tokio::test]
async fn recovers_after_transient_stale_cycles() {
    let (client, slave) = open_client().await;
    let base = seq_after_open(&slave);
    slave.lock().unwrap().silence(Duration::from_millis(3));
    send_nop(&client)
        .await
        .expect("send should recover after the stale burst");
    let s = slave.lock().unwrap();
    assert_eq!(s.expected_seq, base + 1);
    assert_eq!(s.ack, base);
}

fn post_handshake_reset_count(slave: &Arc<StdMutex<Slave>>) -> usize {
    let s = slave.lock().unwrap();
    let after_handshake = s
        .sent_log
        .iter()
        .position(|(_, cmd)| *cmd != Cmd::Reset)
        .unwrap_or(s.sent_log.len());
    s.sent_log[after_handshake..]
        .iter()
        .filter(|(_, cmd)| *cmd == Cmd::Reset)
        .count()
}

#[tokio::test]
async fn inflight_held_across_stale_recovers_without_reset() {
    let (link, slave) = slave_pair();
    slave.lock().unwrap().fpga_state = 0xAB;
    let client = Client::open_bus(&geometry(1), link, ClientConfig::default())
        .await
        .unwrap();
    let base = seq_after_open(&slave);
    slave.lock().unwrap().silence(Duration::from_millis(40));

    let fut = client
        .send_broadcast(&Datagram::no_payload(Cmd::ReadFpgaState))
        .await
        .unwrap();
    assert_eq!(
        first_bytes(&fut.await.unwrap()),
        [0xAB],
        "held in-flight must recover after the stale burst, not time out"
    );
    let s = slave.lock().unwrap();
    assert_eq!(
        s.expected_seq,
        base + 1,
        "the open sequence plus one command, each sent once"
    );
    assert_eq!(s.ack, base);
    drop(s);
    assert_eq!(
        post_handshake_reset_count(&slave),
        0,
        "no Reset escalation when the held front still matches expected_seq"
    );
}

#[tokio::test]
async fn streaming_holds_window_across_stale_and_recovers() {
    let (link, slave) = slave_pair();
    slave.lock().unwrap().fpga_state = 0xAB;
    let client = Client::open_bus(
        &geometry(1),
        link,
        ClientConfig {
            ack_timeout: Duration::from_millis(10),
            max_inflight: NonZeroUsize::new(8).unwrap(),
            max_resync_rounds: NonZeroU32::new(8).unwrap(),
            low_latency: false,
            validate_state: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let base = seq_after_open(&slave);
    slave.lock().unwrap().silence(Duration::from_millis(30));

    let mut futs = Vec::new();
    for _ in 0..8 {
        futs.push(
            client
                .send_broadcast(&Datagram::no_payload(Cmd::ReadFpgaState))
                .await
                .unwrap(),
        );
    }
    for f in futs {
        assert_eq!(
            first_bytes(&f.await.unwrap()),
            [0xAB],
            "every held in-flight must recover after the stale burst"
        );
    }
    assert_eq!(slave.lock().unwrap().expected_seq, base + 8);
    assert_eq!(
        post_handshake_reset_count(&slave),
        0,
        "no Reset escalation needed"
    );
}

#[tokio::test]
async fn frozen_ahead_desync_recovers_via_reset_resync() {
    let (link, slave) = slave_pair();
    slave.lock().unwrap().fpga_state = 0xCD;
    let client = Client::open_bus(&geometry(1), link, ClientConfig::default())
        .await
        .unwrap();
    slave.lock().unwrap().expected_seq = 200;

    let fut = client
        .send_broadcast(&Datagram::no_payload(Cmd::ReadFpgaState))
        .await
        .unwrap();
    assert_eq!(
        first_bytes(&fut.await.unwrap()),
        [0xCD],
        "Reset re-sync must recover the desync instead of waiting for SEQ wraparound"
    );
    assert!(
        post_handshake_reset_count(&slave) > 0,
        "expected a Reset escalation after the handshake"
    );
}

#[tokio::test]
async fn close_resolves_pending_with_rt_closed() {
    let (client, slave) = open_client().await;
    slave.lock().unwrap().drop_next = u32::MAX;
    let f = client
        .send_broadcast(&Datagram::no_payload(Cmd::ReadFpgaState))
        .await
        .unwrap();
    let closed = client.close().await;
    assert!(
        matches!(closed, Err(Error::Timeout { .. })),
        "close must report the stop frame the device dropped, got {closed:?}",
    );
    let err = f.await.unwrap_err();
    assert!(
        matches!(err, Error::DriverClosed) || matches!(err, Error::Timeout { .. }),
        "expected DriverClosed or Timeout, got {err:?}",
    );
}

#[tokio::test]
async fn open_rejects_oversize_max_inflight() {
    let (link, _slave) = slave_pair();
    let res = Client::open_bus(
        &geometry(1),
        link,
        ClientConfig {
            ack_timeout: Duration::from_millis(10),
            max_inflight: NonZeroUsize::new(MAX_INFLIGHT + 1).unwrap(),
            max_resync_rounds: NonZeroU32::new(8).unwrap(),
            low_latency: false,
            validate_state: true,
            ..Default::default()
        },
    )
    .await;
    assert!(matches!(res, Err(Error::InvalidPayload(_))));
}

#[tokio::test]
async fn open_rejects_zero_devices() {
    let (link, _slaves) = slaves_pair(0);
    let res = Client::open_bus(&geometry(0), link, ClientConfig::default()).await;
    assert!(matches!(res, Err(Error::InvalidPayload(_))));
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
    let client = Client::open_bus(&geometry(1), link, config).await.unwrap();
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
    let (client, slave) = open_client().await;
    arm_strict_silencer(&client).await;

    assert!(matches!(
        build_too_fast_pattern(&client),
        Err(Error::SilencerConstraint { .. })
    ));

    slave.lock().unwrap().silence_forever();
    assert!(matches!(
        send_nop(&client).await.unwrap_err(),
        Error::Timeout { .. }
    ));

    assert!(
        build_too_fast_pattern(&client).is_ok(),
        "desynced mirror must stop pre-checking until the next Clear/reopen"
    );
}

#[tokio::test]
async fn raw_send_failure_desyncs_the_mirror() {
    let (client, slave) = open_client().await;
    arm_strict_silencer(&client).await;

    assert!(matches!(
        build_too_fast_pattern(&client),
        Err(Error::SilencerConstraint { .. })
    ));

    slave.lock().unwrap().silence_forever();
    let datagrams = client.datagram_builder().push(Nop).build().unwrap();
    for frame in &datagrams {
        let future = client.send(frame).await.unwrap();
        assert!(matches!(future.await.unwrap_err(), Error::Timeout { .. }));
    }

    assert!(
        build_too_fast_pattern(&client).is_ok(),
        "awaiting a failed raw send must desync the mirror just like send_checked"
    );
}

#[tokio::test]
async fn raw_send_device_error_desyncs_the_mirror() {
    let (client, _slave) = open_client().await;
    arm_strict_silencer(&client).await;

    assert!(matches!(
        build_too_fast_pattern(&client),
        Err(Error::SilencerConstraint { .. })
    ));

    let datagrams = client.datagram_builder().push(FailingCmd).build().unwrap();
    for frame in &datagrams {
        let response = client.send(frame).await.unwrap().await.unwrap();
        assert_eq!(response.status(), [ERR_INVALID_DATA]);
    }

    assert!(
        build_too_fast_pattern(&client).is_ok(),
        "a device error must desync the mirror even when the caller never calls check()"
    );
}

#[tokio::test]
async fn read_replies_never_count_as_device_errors() {
    let (client, slave) = open_client().await;
    arm_strict_silencer(&client).await;
    {
        let mut s = slave.lock().unwrap();
        s.fpga_state = 0x80;
        s.error_detail = 0x7F;
    }

    assert_eq!(client.read_fpga_state().await.unwrap()[0].0, 0x80);
    assert_eq!(client.read_error_detail().await.unwrap(), [0x7F]);

    assert!(
        matches!(
            build_too_fast_pattern(&client),
            Err(Error::SilencerConstraint { .. })
        ),
        "a nonzero read reply is a value, not an ack error, so the mirror must stay synced"
    );
}

#[tokio::test]
async fn validation_opt_out_keeps_the_response_future_mirror_free() {
    let (link, slave) = slave_pair();
    let client = Client::open_bus(
        &geometry(1),
        link,
        ClientConfig {
            validate_state: false,
            ..ClientConfig::default()
        },
    )
    .await
    .unwrap();
    assert!(client.mirror_for_response().is_none());

    slave.lock().unwrap().silence_forever();
    let datagrams = client.datagram_builder().push(Nop).build().unwrap();
    for frame in &datagrams {
        let future = client.send(frame).await.unwrap();
        assert!(matches!(future.await.unwrap_err(), Error::Timeout { .. }));
    }

    assert!(
        build_too_fast_pattern(&client).is_ok(),
        "opt-out must stay a no-op on both the build and the completion side"
    );
}

#[tokio::test]
async fn link_failure_returns_queued_slots_to_the_pool() {
    let (inner, slave) = slave_pair();
    let fail = Arc::new(AtomicBool::new(false));
    let link = FailingLink {
        inner,
        fail: Arc::clone(&fail),
        slow_drop: None,
    };
    let max_inflight = NonZeroUsize::new(3).unwrap();
    let client = Client::open_bus(
        &geometry(1),
        link,
        ClientConfig {
            ack_timeout: Duration::from_secs(3600),
            max_inflight,
            ..ClientConfig::default()
        },
    )
    .await
    .unwrap();

    slave.lock().unwrap().drop_next = u32::MAX;
    let inflight = client
        .send_broadcast_exclusive(&Datagram::no_payload(Cmd::ReadErrorDetail))
        .await
        .unwrap();
    let queued = client
        .send_broadcast(&Datagram::no_payload(Cmd::ReadErrorDetail))
        .await
        .unwrap();

    fail.store(true, AtomicOrdering::Relaxed);
    let inflight_err = inflight.await.unwrap_err();
    let queued_err = queued.await.unwrap_err();

    let closed = client.close().await;
    assert!(
        matches!(closed, Err(Error::DriverClosed))
            || closed
                .as_ref()
                .err()
                .is_some_and(link_cause_is::<LinkFailure>),
        "close must report the stop frame the dead link refused, got {closed:?}"
    );
    assert_eq!(
        client.pool.available_permits(),
        max_inflight.get(),
        "every slot must be back in the pool once the RT thread has torn down"
    );
    assert!(
        link_cause_is::<LinkFailure>(&inflight_err),
        "the link failure must reach the caller as a typed cause, got {inflight_err:?}"
    );
    assert!(
        link_cause_is::<LinkFailure>(&queued_err),
        "a command still queued in the channel must be failed with the link error, got {queued_err:?}"
    );
}

#[tokio::test]
async fn sending_after_the_rt_thread_died_fails_instead_of_blocking() {
    let (inner, _slave) = slave_pair();
    let fail = Arc::new(AtomicBool::new(false));
    let entered_drop = Arc::new(AtomicBool::new(false));
    let link = FailingLink {
        inner,
        fail: Arc::clone(&fail),
        slow_drop: Some(Arc::clone(&entered_drop)),
    };
    let max_inflight = NonZeroUsize::new(1).unwrap();
    let client = Client::open_bus(
        &geometry(1),
        link,
        ClientConfig {
            max_inflight,
            ..ClientConfig::default()
        },
    )
    .await
    .unwrap();

    fail.store(true, AtomicOrdering::Relaxed);
    while !entered_drop.load(AtomicOrdering::Acquire) {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    for _ in 0..4 {
        let err = tokio::time::timeout(Duration::from_secs(5), send_nop(&client))
            .await
            .expect("a send must not block once the RT thread is gone")
            .unwrap_err();
        assert!(
            matches!(err, Error::DriverClosed) || link_cause_is::<LinkFailure>(&err),
            "{err:?}"
        );
    }

    let closed = tokio::time::timeout(Duration::from_secs(5), client.close())
        .await
        .expect("close must return once the RT thread is gone");
    assert!(
        matches!(closed, Err(Error::DriverClosed))
            || closed
                .as_ref()
                .err()
                .is_some_and(link_cause_is::<LinkFailure>),
        "{closed:?}"
    );
    assert_eq!(
        client.pool.available_permits(),
        max_inflight.get(),
        "no slot may be lost to a send that raced the RT thread teardown"
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
    let client = Client::open_bus(&geometry(1), link, config).await.unwrap();

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
    let client = Client::open_bus(&geometry(2), link, ClientConfig::default())
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
async fn stop_mutes_via_a_null_pattern() {
    let (client, slave) = open_client().await;

    client.stop().await.unwrap();

    let s = slave.lock().unwrap();
    assert!(s.muted, "null pattern zeroes every emission");
    assert!(
        s.sent_log
            .iter()
            .any(|(_, cmd)| *cmd == Cmd::WritePatternFused)
    );
    assert!(
        !s.sent_log.iter().any(|(_, cmd)| *cmd == Cmd::SetOutputMask),
        "stop must not touch the output mask"
    );
}

#[tokio::test]
async fn stop_leaves_the_link_synced_for_later_frames() {
    let (client, slave) = open_client().await;

    client.stop().await.unwrap();
    client.read_error_detail().await.unwrap();

    let s = slave.lock().unwrap();
    assert_eq!(s.ack, s.expected_seq.wrapping_sub(1));
}

fn distinct_pattern_frames(slave: &Arc<StdMutex<Slave>>) -> usize {
    let mut seqs: Vec<u8> = slave
        .lock()
        .unwrap()
        .sent_log
        .iter()
        .filter(|(_, cmd)| *cmd == Cmd::WritePatternFused)
        .map(|(seq, _)| *seq)
        .collect();
    seqs.sort_unstable();
    seqs.dedup();
    seqs.len()
}

#[tokio::test]
async fn close_mutes_before_it_joins_the_rt_thread() {
    let (client, slave) = open_client().await;

    client.close().await.unwrap();

    assert!(
        slave.lock().unwrap().muted,
        "close must leave every emission at zero"
    );
    assert_eq!(distinct_pattern_frames(&slave), 1);
}

#[tokio::test]
async fn close_joins_the_rt_thread_even_when_the_stop_frame_fails() {
    let (link, slave) = slave_pair();
    let max_inflight = NonZeroUsize::new(3).unwrap();
    let client = Client::open_bus(
        &geometry(1),
        link,
        ClientConfig {
            max_inflight,
            ..ClientConfig::default()
        },
    )
    .await
    .unwrap();
    slave.lock().unwrap().drop_next = u32::MAX;

    let closed = client.close().await;

    assert!(
        matches!(closed, Err(Error::Timeout { .. })),
        "close must surface the stop failure, got {closed:?}"
    );
    assert_eq!(
        client.pool.available_permits(),
        max_inflight.get(),
        "a failed stop must not keep the RT thread from being joined"
    );
    assert!(matches!(
        send_nop(&client).await.unwrap_err(),
        Error::DriverClosed
    ));
}

#[tokio::test]
async fn a_second_close_does_not_send_another_stop() {
    let (client, slave) = open_client().await;

    client.close().await.unwrap();
    client.close().await.unwrap();

    assert_eq!(
        distinct_pattern_frames(&slave),
        1,
        "the stop sequence must run once per client"
    );
}

#[tokio::test]
async fn read_telemetry_returns_every_counter() {
    let (client, slave) = open_client().await;
    {
        let mut s = slave.lock().unwrap();
        s.telemetry[Telemetry::FifoDrop.as_u8() as usize] = 7;
        s.telemetry[Telemetry::Failsafe.as_u8() as usize] = 3;
        s.telemetry[Telemetry::Processed.as_u8() as usize] = 70_000;
    }

    let counters = client.read_telemetry().await.unwrap();
    assert_eq!(counters.len(), 1);
    assert_eq!(counters[0].get(Telemetry::FifoDrop), 7);
    assert_eq!(counters[0].get(Telemetry::Failsafe), 3);
    assert_eq!(counters[0].get(Telemetry::Processed), 70_000);
}

#[tokio::test]
async fn a_short_read_reply_is_an_unexpected_reply() {
    let (client, slave) = open_client().await;
    slave.lock().unwrap().short_reads = true;
    assert!(matches!(
        client.read_telemetry().await,
        Err(Error::UnexpectedReply { device: 0 })
    ));
    assert!(matches!(
        client.read_firmware_version().await,
        Err(Error::UnexpectedReply { device: 0 })
    ));
}

#[tokio::test]
async fn read_telemetry_returns_sync_resync_count() {
    let (client, slave) = open_client().await;
    slave.lock().unwrap().telemetry[Telemetry::SyncResync.as_u8() as usize] = 5;

    assert_eq!(
        client.read_telemetry().await.unwrap()[0].get(Telemetry::SyncResync),
        5
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

#[derive(Default)]
struct CloseTracker {
    closes: AtomicUsize,
    close_fails: AtomicBool,
    send_fails: AtomicBool,
}

impl CloseTracker {
    fn closes(&self) -> usize {
        self.closes.load(AtomicOrdering::Acquire)
    }
}

struct TrackedLink {
    inner: LoopbackLink,
    tracker: Arc<CloseTracker>,
    stats: BusStats,
}

fn tracked_pair() -> (TrackedLink, Arc<CloseTracker>) {
    let (inner, _slave) = slave_pair();
    let tracker = Arc::new(CloseTracker::default());
    (
        TrackedLink {
            inner,
            tracker: Arc::clone(&tracker),
            stats: BusStats::default(),
        },
        tracker,
    )
}

impl Bus for TrackedLink {
    type Error = LinkFailure;

    fn num_devices(&self) -> usize {
        self.inner.num_devices()
    }

    fn stats(&self) -> BusStats {
        self.stats.clone()
    }

    fn next_msg_id(&self) -> u16 {
        self.inner.next_msg_id()
    }

    fn send(&mut self, frames: &[FrameBuf]) -> Result<u16, Self::Error> {
        if self.tracker.send_fails.load(AtomicOrdering::Acquire) {
            return Err(LinkFailure);
        }
        Ok(self.inner.send(frames).expect("loopback never fails"))
    }

    fn heartbeat(&mut self) -> Result<u16, Self::Error> {
        Ok(self.inner.heartbeat().expect("loopback never fails"))
    }

    fn try_recv(&mut self) -> Result<Option<Reply>, Self::Error> {
        Ok(self.inner.try_recv().expect("loopback never fails"))
    }

    fn wait_readable(&mut self, deadline: Instant) -> Result<bool, Self::Error> {
        Ok(self
            .inner
            .wait_readable(deadline)
            .expect("loopback never fails"))
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        self.tracker.closes.fetch_add(1, AtomicOrdering::AcqRel);
        if self.tracker.close_fails.load(AtomicOrdering::Acquire) {
            return Err(LinkFailure);
        }
        Ok(())
    }
}

#[tokio::test]
async fn close_calls_the_link_close_exactly_once() {
    let (link, tracker) = tracked_pair();
    let client = Client::open_bus(&geometry(1), link, ClientConfig::default())
        .await
        .unwrap();
    client.close().await.unwrap();
    assert_eq!(tracker.closes(), 1);
    client.close().await.unwrap();
    drop(client);
    assert_eq!(tracker.closes(), 1);
}

#[tokio::test]
async fn dropping_the_client_still_closes_the_link() {
    let (link, tracker) = tracked_pair();
    let (connector, driver) = super::spawn_driver(link);
    let client = Client::open(&geometry(1), connector, ClientConfig::default())
        .await
        .unwrap();
    drop(client);
    driver.join().unwrap().unwrap();
    assert_eq!(tracker.closes(), 1);
}

#[tokio::test]
async fn link_close_failure_surfaces_from_client_close() {
    let (link, tracker) = tracked_pair();
    let client = Client::open_bus(&geometry(1), link, ClientConfig::default())
        .await
        .unwrap();
    tracker.close_fails.store(true, AtomicOrdering::Release);
    let closed = client.close().await;
    assert!(link_cause_is::<LinkFailure>(&closed.unwrap_err()));
    assert_eq!(tracker.closes(), 1);
}

#[tokio::test]
async fn the_link_is_closed_even_when_the_handshake_fails() {
    let (link, tracker) = tracked_pair();
    tracker.send_fails.store(true, AtomicOrdering::Release);
    let opened = Client::open_bus(&geometry(1), link, ClientConfig::default()).await;
    assert!(link_cause_is::<LinkFailure>(
        &opened.err().expect("open fails")
    ));
    assert_eq!(tracker.closes(), 1);
}

#[tokio::test]
async fn a_link_rejected_by_the_device_count_check_is_still_closed() {
    let (link, tracker) = tracked_pair();
    let (connector, driver) = super::spawn_driver(link);
    let opened = Client::open(&geometry(2), connector, ClientConfig::default()).await;
    assert!(matches!(opened, Err(Error::InvalidPayload(_))));
    driver.join().unwrap().unwrap();
    assert_eq!(tracker.closes(), 1);
}

#[tokio::test]
async fn geometry_is_reachable_through_the_client() {
    let (link, _tracker) = tracked_pair();
    let client = Client::open_bus(&geometry(1), link, ClientConfig::default())
        .await
        .unwrap();
    assert_eq!(client.geometry().num_devices(), 1);
    assert_eq!(
        client.geometry().iter().next().unwrap().num_transducers(),
        geometry(1).iter().next().unwrap().num_transducers()
    );
    client.close().await.unwrap();
}

#[tokio::test]
async fn bus_stats_are_reachable_through_the_client() {
    let (link, _tracker) = tracked_pair();
    let client = Client::open_bus(&geometry(1), link, ClientConfig::default())
        .await
        .unwrap();
    let stats = client.bus_stats();
    assert!(stats.frames() > 0);
    let before = stats.acked_frames();
    send_nop(&client).await.unwrap();
    assert!(client.bus_stats().acked_frames() > before);
    assert_eq!(
        client.bus_stats().frames(),
        client.bus_stats().acked_frames()
    );
    client.close().await.unwrap();
}
