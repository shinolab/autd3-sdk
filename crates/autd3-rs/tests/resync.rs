use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use autd3_cpu_fw::proto::Mode;
use autd3_cpu_fw::{FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH};
use autd3_rs::Telemetry;
use autd3_rs::commands::SetSilencer;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::link::{CycleOutcome, Link, LinkStats};
use autd3_rs::protocol::{Cmd, RX_FRAME_BYTES, TX_FRAME_BYTES, TxFrame};
use autd3_rs::{Client, ClientConfig};
use autd3_rs_firmware_emulator::{Audit, Fault};

#[derive(Clone)]
struct SharedAudit {
    audit: Arc<Mutex<Audit>>,
    resets: Arc<AtomicUsize>,
    sent: Arc<Mutex<Vec<(u8, Cmd)>>>,
}

impl SharedAudit {
    fn new(n: usize) -> Self {
        Self {
            audit: Arc::new(Mutex::new(Audit::new(
                (0..n).map(|_| Autd3::NUM_TRANSDUCERS),
            ))),
            resets: Arc::new(AtomicUsize::new(0)),
            sent: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn inject(&self, fault: Fault) {
        self.audit.lock().unwrap().inject(fault);
    }

    fn mode(&self, device: usize) -> Mode {
        self.audit.lock().unwrap().device(device).mode()
    }

    fn resets(&self) -> usize {
        self.resets.load(Ordering::Relaxed)
    }

    fn round_trips(&self) -> usize {
        let sent = self.sent.lock().unwrap();
        let mut distinct = 0;
        let mut last = None;
        for frame in sent.iter() {
            if last != Some(*frame) {
                distinct += 1;
                last = Some(*frame);
            }
        }
        distinct
    }

    fn run_device_ahead(&self, device: usize, frames: usize) {
        let mut audit = self.audit.lock().unwrap();
        let device = audit.device_mut(device);
        let mut seq = device.rx().ack.next();
        let mut bytes = [0u8; TX_FRAME_BYTES];
        for _ in 0..frames {
            TxFrame::new(seq, Cmd::Nop).write_to(&mut bytes);
            device.recv(&bytes);
            device.process_pending();
            seq = seq.next();
        }
    }
}

impl Link for SharedAudit {
    type Error = core::convert::Infallible;
    type Checker = <Audit as Link>::Checker;

    fn num_devices(&self) -> usize {
        self.audit.lock().unwrap().num_devices()
    }

    fn stats(&self) -> LinkStats {
        self.audit.lock().unwrap().stats()
    }

    fn state_checker(&self) -> Self::Checker {
        self.audit.lock().unwrap().state_checker()
    }

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Self::Error> {
        if let Some(frame) = tx.first().and_then(|f| TxFrame::parse(f).ok()) {
            if frame.cmd == Cmd::Reset {
                self.resets.fetch_add(1, Ordering::Relaxed);
            }
            self.sent.lock().unwrap().push((frame.seq.get(), frame.cmd));
        }
        self.audit.lock().unwrap().cycle(tx, rx)
    }
}

fn geometry(n: usize) -> Geometry {
    Geometry::new((0..n).map(|_| Autd3::default()).collect())
}

fn resilient_config() -> ClientConfig {
    ClientConfig {
        timeout_cycles: NonZeroU32::new(50).unwrap(),
        max_inflight: NonZeroUsize::new(16).unwrap(),
        max_resync_rounds: NonZeroU32::new(8).unwrap(),
        reset_resend_cycles: NonZeroU32::new(2).unwrap(),
        ..ClientConfig::default()
    }
}

async fn open(link: SharedAudit, n: usize, config: ClientConfig) -> Client {
    Client::open(&geometry(n), link, config).await.unwrap()
}

async fn stream_silencer(client: &Client, rounds: usize) {
    for _ in 0..rounds {
        let frames = client
            .datagram_builder()
            .push(SetSilencer::default())
            .build()
            .unwrap();
        for frame in &frames {
            client.send_checked(frame).await.unwrap();
        }
    }
}

fn assert_real_firmware(client: &Client, versions: &[autd3_rs::FirmwareVersion]) {
    assert_eq!(versions.len(), client.num_devices());
    for (i, v) in versions.iter().enumerate() {
        assert_eq!(
            (v.cpu.major, v.cpu.minor, v.cpu.patch),
            (FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH),
            "device {i} reported a version the vendored firmware does not have"
        );
    }
}

#[tokio::test]
async fn a_skipped_frame_recovers_via_go_back_n() {
    let link = SharedAudit::new(1);
    let client = open(link.clone(), 1, resilient_config()).await;

    let resets_before = link.resets();
    link.inject(Fault {
        drop_frames: 1,
        ..Fault::default()
    });

    let versions = client.read_firmware_version().await.unwrap();
    assert_real_firmware(&client, &versions);
    stream_silencer(&client, 4).await;
    assert_eq!(
        link.resets(),
        resets_before,
        "a single skip must be recovered by go-back-N alone, without a Reset escalation"
    );
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_device_that_ran_ahead_recovers_via_reset_resync() {
    let link = SharedAudit::new(1);
    let client = open(link.clone(), 1, resilient_config()).await;

    let resets_before = link.resets();
    link.run_device_ahead(0, 200);

    let versions = client.read_firmware_version().await.unwrap();
    assert_real_firmware(&client, &versions);
    stream_silencer(&client, 4).await;
    assert!(
        link.resets() > resets_before,
        "a device that ran ahead of the client must be recovered by a Reset escalation"
    );
    client.close().await.unwrap();
}

#[tokio::test]
async fn an_rx_invalid_interval_does_not_lose_a_frame() {
    let link = SharedAudit::new(1);
    let client = open(link.clone(), 1, resilient_config()).await;

    link.inject(Fault {
        invalid_cycles: 5,
        ..Fault::default()
    });

    let versions = client.read_firmware_version().await.unwrap();
    assert_real_firmware(&client, &versions);
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_skip_on_one_device_only_resyncs_every_device() {
    let link = SharedAudit::new(2);
    let client = open(link.clone(), 2, resilient_config()).await;

    link.inject(Fault {
        drop_frames: 1,
        device: Some(1),
        ..Fault::default()
    });

    let versions = client.read_firmware_version().await.unwrap();
    assert_real_firmware(&client, &versions);
    stream_silencer(&client, 4).await;
    client.close().await.unwrap();
}

#[tokio::test]
async fn the_low_latency_handshake_switches_the_real_firmware() {
    let link = SharedAudit::new(1);
    assert_eq!(link.mode(0), Mode::Fifo);

    let client = open(
        link.clone(),
        1,
        ClientConfig {
            low_latency: true,
            ..resilient_config()
        },
    )
    .await;
    assert_eq!(
        link.mode(0),
        Mode::LowLatency,
        "SetMode must have been negotiated during the handshake"
    );

    let versions = client.read_firmware_version().await.unwrap();
    assert_real_firmware(&client, &versions);
    stream_silencer(&client, 4).await;
    client.close().await.unwrap();
}

#[tokio::test]
async fn the_default_config_leaves_the_real_firmware_in_fifo_mode() {
    let link = SharedAudit::new(1);
    let client = open(link.clone(), 1, resilient_config()).await;
    assert_eq!(link.mode(0), Mode::Fifo);
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_new_session_takes_the_real_firmware_back_out_of_low_latency() {
    let link = SharedAudit::new(1);
    let client = open(
        link.clone(),
        1,
        ClientConfig {
            low_latency: true,
            ..resilient_config()
        },
    )
    .await;
    assert_eq!(link.mode(0), Mode::LowLatency);
    client.close().await.unwrap();

    let client = open(link.clone(), 1, resilient_config()).await;
    assert_eq!(
        link.mode(0),
        Mode::Fifo,
        "a low-latency device must return to FIFO without a power cycle"
    );
    let versions = client.read_firmware_version().await.unwrap();
    assert_real_firmware(&client, &versions);
    stream_silencer(&client, 4).await;
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_telemetry_counter_the_firmware_knows_reads_back() {
    let link = SharedAudit::new(1);
    let client = open(link.clone(), 1, resilient_config()).await;

    let counters = client.read_telemetry(Telemetry::Processed).await.unwrap();
    assert_eq!(counters.len(), 1);
    client.close().await.unwrap();
}

#[tokio::test]
async fn the_read_error_detail_sandwich_costs_two_extra_round_trips() {
    let link = SharedAudit::new(1);
    let client = open(link.clone(), 1, resilient_config()).await;

    let before = link.round_trips();
    client.read_telemetry(Telemetry::Processed).await.unwrap();
    assert_eq!(
        link.round_trips() - before,
        3,
        "read_telemetry is ReadErrorDetail + ReadTelemetry + ReadErrorDetail"
    );

    let before = link.round_trips();
    client.read_firmware_version().await.unwrap();
    assert_eq!(
        link.round_trips() - before,
        10,
        "read_firmware_version is 3 CPU + 3 FPGA version + 1 FPGA functions read, sandwiched by 3 ReadErrorDetail"
    );
    client.close().await.unwrap();
}
