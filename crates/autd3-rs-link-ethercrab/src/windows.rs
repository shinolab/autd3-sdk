use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::task::Wake;
use std::time::{Duration, Instant};

use ethercrab::{PduRx, PduTx, ReceiveAction};

use crate::error::EtherCrabLinkError;

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

fn open_capture(
    device: &str,
) -> Result<(pcap::Capture<pcap::Active>, pcap::sendqueue::SendQueue), EtherCrabLinkError> {
    let cap = pcap::Capture::from_device(device)?
        .immediate_mode(true)
        .timeout(-1)
        .open()?
        .setnonblock()?;
    let sq = pcap::sendqueue::SendQueue::new(32 * 1024)?;
    Ok((cap, sq))
}

pub(crate) fn tx_rx_task_blocking(
    device: &str,
    mut pdu_tx: PduTx<'_>,
    mut pdu_rx: PduRx<'_>,
    running: &Arc<AtomicBool>,
    ready: &SyncSender<Result<(), EtherCrabLinkError>>,
) -> Result<(), EtherCrabLinkError> {
    if let Err(e) =
        thread_priority::set_current_thread_priority(thread_priority::ThreadPriority::Os(
            thread_priority::WinAPIThreadPriority::TimeCritical.into(),
        ))
    {
        tracing::warn!("failed to raise tx/rx thread priority: {e:?}");
    }

    let waker = std::task::Waker::from(Arc::new(ThreadWaker::new()));

    let (mut cap, mut sq) = match open_capture(device) {
        Ok(pair) => {
            let _ = ready.send(Ok(()));
            pair
        }
        Err(e) => {
            let _ = ready.send(Err(e));
            return Ok(());
        }
    };

    let mut in_flight = 0usize;

    while running.load(Ordering::Relaxed) {
        pdu_tx.replace_waker(&waker);

        let mut sent_this_iter = 0usize;
        while let Some(frame) = pdu_tx.next_sendable_frame() {
            frame
                .send_blocking(|frame_bytes| {
                    sq.queue(None, frame_bytes)
                        .map_err(|_| ethercrab::error::Error::SendFrame)?;
                    Ok(frame_bytes.len())
                })
                .map_err(io::Error::other)?;
            sent_this_iter += 1;
        }

        if sent_this_iter > 0 {
            sq.transmit(&mut cap, pcap::sendqueue::SendSync::Off)?;
            in_flight += sent_this_iter;
        }

        if in_flight > 0 {
            let deadline = Instant::now() + RX_DRAIN_TIMEOUT;
            while running.load(Ordering::Relaxed) {
                match cap.next_packet() {
                    Ok(packet) => match pdu_rx.receive_frame(packet.data) {
                        Ok(ReceiveAction::Processed) => {
                            in_flight -= 1;
                            if in_flight == 0 {
                                break;
                            }
                        }
                        Ok(ReceiveAction::Ignored) => {}
                        Err(e) => tracing::trace!("skipping unprocessable RX frame: {e}"),
                    },
                    Err(pcap::Error::NoMorePackets | pcap::Error::TimeoutExpired) => {
                        if Instant::now() >= deadline {
                            in_flight = 0;
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
