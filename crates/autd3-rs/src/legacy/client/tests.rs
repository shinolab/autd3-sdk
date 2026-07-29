use core::convert::Infallible;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use autd3_rs_core::geometry::{Autd3, Device, Geometry};
use autd3_rs_core::link::{ConstStateChecker, CycleOutcome, DcClock, Link};
use autd3_rs_core::value::{DcSysTime, ModulationBank};

use crate::commands::{ChangeModulationBank, GpioOut, SetGpioOut};
use crate::legacy::emulator::LegacyAudit;
use crate::legacy::error::{LegacyError, NOT_SUPPORTED_TAG};
use crate::legacy::wire::{Ack, RX_FRAME_BYTES, RxFrame, TX_FRAME_BYTES, Tag, TxFrame};
use crate::value::TransitionMode;

use super::{LegacyClient, LegacyClientConfig};

const MUTE: [u8; 3] = [Tag::Silencer.as_u8(), Tag::Gain.as_u8(), Tag::Clear.as_u8()];

#[derive(Default)]
struct Recorder {
    tags: Vec<u8>,
    last_msg_id: Option<u8>,
}

struct RecordingLink {
    inner: LegacyAudit,
    recorder: Arc<StdMutex<Recorder>>,
    fail_tag: Option<u8>,
}

impl Link for RecordingLink {
    type Error = Infallible;
    type Checker = ConstStateChecker;

    fn num_devices(&self) -> usize {
        self.inner.num_devices()
    }

    fn state_checker(&self) -> Self::Checker {
        self.inner.state_checker()
    }

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Infallible> {
        let head = TxFrame::parse(&tx[0]);
        let tag = head.payload[0];
        {
            let mut recorder = self
                .recorder
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if recorder.last_msg_id != Some(head.header.msg_id) {
                recorder.last_msg_id = Some(head.header.msg_id);
                recorder.tags.push(tag);
            }
        }
        let outcome = self.inner.cycle(tx, rx)?;
        if self.fail_tag == Some(tag) {
            for buf in rx.iter_mut() {
                let frame = RxFrame::parse(*buf);
                RxFrame::new(frame.data, Ack::new(frame.ack.msg_id(), NOT_SUPPORTED_TAG))
                    .write_to(buf);
            }
        }
        Ok(outcome)
    }
}

fn geometry(num_devices: usize) -> Geometry {
    Geometry::new((0..num_devices).map(|_| Autd3::default()).collect())
}

async fn open(fail_tag: Option<u8>) -> (LegacyClient, Arc<StdMutex<Recorder>>) {
    let geometry = geometry(2);
    let recorder = Arc::new(StdMutex::new(Recorder::default()));
    let link = RecordingLink {
        inner: LegacyAudit::new(geometry.iter().map(Device::num_transducers)),
        recorder: Arc::clone(&recorder),
        fail_tag,
    };
    let client = LegacyClient::open(&geometry, link, LegacyClientConfig::default())
        .await
        .unwrap();
    recorder
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .tags
        .clear();
    (client, recorder)
}

fn tags(recorder: &Arc<StdMutex<Recorder>>) -> Vec<u8> {
    recorder
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .tags
        .clone()
}

#[tokio::test]
async fn close_mutes_before_it_tears_the_rt_thread_down() {
    let (client, recorder) = open(None).await;
    client.close().await.unwrap();
    assert_eq!(tags(&recorder), MUTE);
}

#[tokio::test]
async fn a_failing_silencer_does_not_skip_the_stop_and_the_clear() {
    let (client, recorder) = open(Some(Tag::Silencer.as_u8())).await;
    let err = client.close().await.unwrap_err();
    assert!(matches!(
        err,
        LegacyError::Device {
            code: NOT_SUPPORTED_TAG,
            ..
        }
    ));
    assert_eq!(tags(&recorder), MUTE);
}

#[tokio::test]
async fn a_failing_stop_does_not_skip_the_clear() {
    let (client, recorder) = open(Some(Tag::Gain.as_u8())).await;
    let err = client.close().await.unwrap_err();
    assert!(matches!(
        err,
        LegacyError::Device {
            code: NOT_SUPPORTED_TAG,
            ..
        }
    ));
    assert_eq!(tags(&recorder), MUTE);
}

#[tokio::test]
async fn dropping_without_close_still_mutes() {
    let (client, recorder) = open(None).await;
    drop(client);
    assert_eq!(tags(&recorder), MUTE);
}

#[tokio::test]
async fn dropping_after_close_does_not_mute_twice() {
    let (client, recorder) = open(None).await;
    client.close().await.unwrap();
    drop(client);
    assert_eq!(tags(&recorder), MUTE);
}

#[tokio::test]
async fn a_second_close_neither_resends_nor_fails() {
    let (client, recorder) = open(None).await;
    client.close().await.unwrap();
    client.close().await.unwrap();
    assert_eq!(tags(&recorder), MUTE);
}

#[tokio::test]
async fn sending_after_close_is_rejected() {
    let (client, recorder) = open(None).await;
    client.close().await.unwrap();
    assert!(matches!(
        client.stop().await.unwrap_err(),
        LegacyError::Closed
    ));
    assert!(matches!(
        client.read_fpga_state().await.unwrap_err(),
        LegacyError::Closed
    ));
    assert_eq!(tags(&recorder), MUTE);
}

#[tokio::test]
async fn a_concurrent_send_never_interleaves_with_the_mute_sequence() {
    let (client, recorder) = open(None).await;
    let client = Arc::new(client);
    let sender = {
        let client = Arc::clone(&client);
        tokio::spawn(async move {
            for _ in 0..64 {
                if client.stop().await.is_err() {
                    break;
                }
            }
        })
    };
    client.close().await.unwrap();
    sender.await.unwrap();

    let tags = tags(&recorder);
    let (head, tail) = tags.split_at(tags.len() - MUTE.len());
    assert_eq!(tail, MUTE);
    assert!(head.iter().all(|&tag| tag == Tag::Gain.as_u8()));
}

struct DcLink {
    inner: LegacyAudit,
    dc_clock: DcClock,
}

impl Link for DcLink {
    type Error = Infallible;
    type Checker = ConstStateChecker;

    fn num_devices(&self) -> usize {
        self.inner.num_devices()
    }

    fn state_checker(&self) -> Self::Checker {
        self.inner.state_checker()
    }

    fn dc_clock(&self) -> Option<DcClock> {
        Some(self.dc_clock.clone())
    }

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Infallible> {
        self.inner.cycle(tx, rx)
    }
}

const TRANSITION_TIME_NS: u64 = 3125 * 4096;
const BUS_OFFSET_NS: i64 = 3125 * 7;

async fn open_with_dc_clock(offset_ns: i64) -> LegacyClient {
    let geometry = geometry(1);
    let dc_clock = DcClock::new();
    dc_clock.observe_against(
        DcSysTime::from_nanos(1_000_000_000u64.saturating_add_signed(offset_ns)),
        DcSysTime::from_nanos(1_000_000_000),
    );
    let link = DcLink {
        inner: LegacyAudit::new(geometry.iter().map(Device::num_transducers)),
        dc_clock,
    };
    LegacyClient::open(&geometry, link, LegacyClientConfig::default())
        .await
        .unwrap()
}

fn sys_time_payload(client: &LegacyClient) -> Vec<u8> {
    let mut builder = client.datagram_builder();
    builder.push(ChangeModulationBank {
        bank: ModulationBank::B1,
        transition_mode: TransitionMode::SysTime {
            time: DcSysTime::from_nanos(TRANSITION_TIME_NS),
            margin: None,
        },
    });
    let frames = builder.build().unwrap();
    assert_eq!(frames.len(), 1);
    frames.frame(0).unwrap().frames()[0].payload.to_vec()
}

fn sys_time_eq_payload(client: &LegacyClient) -> Vec<u8> {
    let mut builder = client.datagram_builder();
    builder.push(SetGpioOut {
        outputs: [
            GpioOut::SysTimeEq(DcSysTime::from_nanos(TRANSITION_TIME_NS)),
            GpioOut::Off,
            GpioOut::Off,
            GpioOut::Off,
        ],
    });
    let frames = builder.build().unwrap();
    assert_eq!(frames.len(), 1);
    frames.frame(0).unwrap().frames()[0].payload.to_vec()
}

#[tokio::test]
async fn a_dc_clock_retimes_sys_time_onto_the_bus_clock() {
    let client = open_with_dc_clock(BUS_OFFSET_NS).await;
    assert_eq!(client.dc_offset_ns(), BUS_OFFSET_NS);

    let payload = sys_time_payload(&client);
    let retimed = TRANSITION_TIME_NS.saturating_add_signed(BUS_OFFSET_NS);
    assert_eq!(&payload[8..16], &retimed.to_le_bytes());

    client.close().await.unwrap();
}

#[tokio::test]
async fn a_dc_clock_retimes_sys_time_eq_before_it_is_scaled() {
    let client = open_with_dc_clock(BUS_OFFSET_NS).await;

    let payload = sys_time_eq_payload(&client);
    let word = u64::from_le_bytes(payload[8..16].try_into().unwrap());
    let retimed = TRANSITION_TIME_NS.saturating_add_signed(BUS_OFFSET_NS);
    assert_eq!(word >> 56, 0x60);
    assert_eq!(word & 0x00FF_FFFF_FFFF_FFFF, ((retimed / 3125) << 6) >> 9);

    client.close().await.unwrap();
}

#[tokio::test]
async fn a_link_without_a_dc_clock_leaves_the_requested_times_alone() {
    let (client, _recorder) = open(None).await;
    assert_eq!(client.dc_offset_ns(), 0);

    assert_eq!(
        &sys_time_payload(&client)[8..16],
        &TRANSITION_TIME_NS.to_le_bytes()
    );
    let word = u64::from_le_bytes(sys_time_eq_payload(&client)[8..16].try_into().unwrap());
    assert_eq!(
        word & 0x00FF_FFFF_FFFF_FFFF,
        ((TRANSITION_TIME_NS / 3125) << 6) >> 9
    );

    client.close().await.unwrap();
}
