use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::task::Wake;
use std::time::{Duration, Instant};

use ethercrab::{PduRx, PduTx, ReceiveAction};

use crate::error::EtherCrabLinkError;
use crate::osal::frame::{EchoFilter, MAX_FRAME_BYTES, normalize_source_mac};

const RX_DRAIN_TIMEOUT: Duration = Duration::from_millis(100);

struct ThreadWaker {
    thread: std::thread::Thread,
}

impl ThreadWaker {
    fn new() -> Self {
        Self {
            thread: std::thread::current(),
        }
    }
}

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.thread.unpark();
    }
}

struct Capture {
    cap: pcap::Capture<pcap::Active>,
    queue: pcap::sendqueue::SendQueue,
    echo: Option<EchoFilter>,
    scratch: Vec<u8>,
}

impl Capture {
    fn open(device: &str) -> Result<Self, EtherCrabLinkError> {
        let cap = pcap::Capture::from_device(device)?
            .immediate_mode(true)
            .timeout(-1)
            .open()?
            .setnonblock()?;
        let echo = match cap.direction(pcap::Direction::In) {
            Ok(()) => None,
            Err(e) => {
                tracing::debug!(
                    "the driver keeps looping sent frames back; they are rejected by \
                     comparison: {e}"
                );
                Some(EchoFilter::new())
            }
        };
        Ok(Self {
            cap,
            queue: pcap::sendqueue::SendQueue::new(32 * 1024)?,
            echo,
            scratch: vec![0u8; MAX_FRAME_BYTES],
        })
    }

    fn queue(&mut self, frame: &[u8]) -> Result<(), pcap::Error> {
        if let Some(echo) = self.echo.as_mut() {
            echo.record(frame);
        }
        self.queue.queue(None, frame)
    }

    fn transmit(&mut self) -> Result<(), pcap::Error> {
        self.queue
            .transmit(&mut self.cap, pcap::sendqueue::SendSync::Off)
    }

    fn next_frame(&mut self) -> Result<Option<&[u8]>, pcap::Error> {
        let data = self.cap.next_packet()?.data;
        if self.echo.as_mut().is_some_and(|echo| echo.take(data)) {
            return Ok(None);
        }
        Ok(Some(normalize_source_mac(data, &mut self.scratch)))
    }
}

pub(crate) fn tx_rx_task_blocking(
    device: &str,
    mut pdu_tx: PduTx<'_>,
    mut pdu_rx: PduRx<'_>,
    running: &Arc<AtomicBool>,
    ready: &SyncSender<Result<(), EtherCrabLinkError>>,
    tuning: crate::osal::thread::PumpTuning,
) -> Result<(), EtherCrabLinkError> {
    if tuning.priority.is_some() {
        crate::osal::thread::apply_thread_tuning(tuning.priority, tuning.policy, tuning.affinity);
    } else {
        if let Err(e) =
            thread_priority::set_current_thread_priority(thread_priority::ThreadPriority::Os(
                thread_priority::WinAPIThreadPriority::TimeCritical.into(),
            ))
        {
            tracing::warn!("failed to raise tx/rx thread priority: {e:?}");
        }
        crate::osal::thread::apply_thread_tuning(None, tuning.policy, tuning.affinity);
    }

    let waker = std::task::Waker::from(Arc::new(ThreadWaker::new()));

    let mut capture = match Capture::open(device) {
        Ok(capture) => {
            let _ = ready.send(Ok(()));
            capture
        }
        Err(e) => {
            let _ = ready.send(Err(e));
            return Ok(());
        }
    };

    let mut inflight = 0usize;

    while running.load(Ordering::Relaxed) {
        pdu_tx.replace_waker(&waker);

        let mut sent_this_iter = 0usize;
        while let Some(frame) = pdu_tx.next_sendable_frame() {
            frame
                .send_blocking(|frame_bytes| {
                    capture
                        .queue(frame_bytes)
                        .map_err(|_| ethercrab::error::Error::SendFrame)?;
                    Ok(frame_bytes.len())
                })
                .map_err(io::Error::other)?;
            sent_this_iter += 1;
        }

        if sent_this_iter > 0 {
            capture.transmit()?;
            inflight += sent_this_iter;
        }

        if inflight > 0 {
            let deadline = Instant::now() + RX_DRAIN_TIMEOUT;
            while running.load(Ordering::Relaxed) {
                match capture.next_frame() {
                    Ok(None) => {}
                    Ok(Some(frame)) => match pdu_rx.receive_frame(frame) {
                        Ok(ReceiveAction::Processed) => {
                            inflight -= 1;
                            if inflight == 0 {
                                break;
                            }
                        }
                        Ok(ReceiveAction::Ignored) => {}
                        Err(e) => tracing::trace!("skipping unprocessable RX frame: {e}"),
                    },
                    Err(pcap::Error::NoMorePackets | pcap::Error::TimeoutExpired) => {
                        if Instant::now() >= deadline {
                            inflight = 0;
                            break;
                        }
                    }
                    Err(e) => return Err(io::Error::other(e).into()),
                }
            }
        } else {
            std::thread::park();
            if pdu_tx.should_exit() {
                break;
            }
        }
    }

    Ok(())
}
