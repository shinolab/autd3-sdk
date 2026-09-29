use std::collections::VecDeque;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::Telemetry;
use crate::commands::SetSilencer;
use crate::geometry::{Autd3, Geometry};
use crate::protocol::{Cmd, FRAME_BYTES_MAX, TxFrame};
use crate::transport::Bus;
use crate::udp::Reply;
use crate::{Client, ClientConfig};
use autd3_cpu_fw::proto::Mode;
use autd3_cpu_fw::{FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH};
use autd3_rs_firmware_emulator::{Audit, AuditReply, Fault};

#[derive(Clone)]
struct SharedAudit {
    audit: Arc<Mutex<Audit>>,
    resets: Arc<AtomicUsize>,
    sent: Arc<Mutex<Vec<(u8, Cmd)>>>,
    queue: VecDeque<AuditReply>,
    msg_id: u16,
}

impl SharedAudit {
    fn new(n: usize) -> Self {
        Self {
            audit: Arc::new(Mutex::new(Audit::new(
                (0..n).map(|_| Autd3::NUM_TRANSDUCERS),
            ))),
            resets: Arc::new(AtomicUsize::new(0)),
            sent: Arc::new(Mutex::new(Vec::new())),
            queue: VecDeque::new(),
            msg_id: 0,
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
        let mut seq = crate::protocol::Seq::new(device.reply().ack).next();
        let mut bytes = [0u8; FRAME_BYTES_MAX];
        for _ in 0..frames {
            TxFrame::new(seq, Cmd::Nop).write_to(&mut bytes);
            let _ = device.recv(&bytes, 0);
            device.process_pending();
            seq = seq.next();
        }
    }
}

impl Bus for SharedAudit {
    type Error = core::convert::Infallible;

    fn num_devices(&self) -> usize {
        self.audit.lock().unwrap().num_devices()
    }

    fn next_msg_id(&self) -> u16 {
        self.msg_id.wrapping_add(1)
    }

    fn send(&mut self, frames: &[[u8; FRAME_BYTES_MAX]]) -> Result<u16, Self::Error> {
        if let Some(frame) = frames.first().and_then(|f| TxFrame::parse(f).ok()) {
            if frame.cmd == Cmd::Reset {
                self.resets.fetch_add(1, Ordering::Relaxed);
            }
            self.sent.lock().unwrap().push((frame.seq.get(), frame.cmd));
        }
        self.msg_id = self.msg_id.wrapping_add(1);
        let refs: Vec<&[u8]> = frames.iter().map(|f| &f[..]).collect();
        let replies = self.audit.lock().unwrap().send(&refs, self.msg_id);
        self.queue.extend(replies);
        Ok(self.msg_id)
    }

    fn heartbeat(&mut self) -> Result<u16, Self::Error> {
        self.msg_id = self.msg_id.wrapping_add(1);
        let replies = self.audit.lock().unwrap().heartbeat(self.msg_id);
        self.queue.extend(replies);
        Ok(self.msg_id)
    }

    fn recv(&mut self, deadline: Instant) -> Result<Option<Reply>, Self::Error> {
        if let Some(r) = self.queue.pop_front() {
            return Ok(Some(Reply::new(
                r.device,
                r.msg_id,
                r.reply.ack,
                r.reply.status,
                0x09,
                r.reply.data(),
            )));
        }
        let now = Instant::now();
        if deadline > now {
            std::thread::sleep(deadline - now);
        }
        Ok(None)
    }
}

fn geometry(n: usize) -> Geometry {
    Geometry::new((0..n).map(|_| Autd3::default()).collect())
}

fn resilient_config() -> ClientConfig {
    ClientConfig {
        ack_timeout: Duration::from_millis(20),
        max_inflight: NonZeroUsize::new(7).unwrap(),
        max_resync_rounds: NonZeroU32::new(8).unwrap(),
        ..ClientConfig::default()
    }
}

async fn open(link: SharedAudit, n: usize, config: ClientConfig) -> Client {
    Client::open_bus(&geometry(n), link, config).await.unwrap()
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

fn assert_real_firmware(client: &Client, versions: &[crate::FirmwareVersion]) {
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
async fn lost_replies_do_not_lose_a_frame() {
    let link = SharedAudit::new(1);
    let client = open(link.clone(), 1, resilient_config()).await;

    link.inject(Fault {
        drop_replies: 5,
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

    let counters = client.read_telemetry().await.unwrap();
    assert_eq!(counters.len(), 1);
    assert!(counters[0].get(Telemetry::Processed) > 0);
    client.close().await.unwrap();
}

#[tokio::test]
async fn every_read_is_a_single_round_trip() {
    let link = SharedAudit::new(1);
    let client = open(link.clone(), 1, resilient_config()).await;

    let before = link.round_trips();
    client.read_telemetry().await.unwrap();
    assert_eq!(link.round_trips() - before, 1);

    let before = link.round_trips();
    client.read_firmware_version().await.unwrap();
    assert_eq!(link.round_trips() - before, 1);
    client.close().await.unwrap();
}
