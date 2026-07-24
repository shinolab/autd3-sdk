use ethercrab::{PduRx, PduTx};

use crate::error::EtherCrabLinkError;
use crate::osal::thread::PumpTuning;

const TX_RX_EXIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

#[cfg(target_os = "linux")]
use crate::osal::linux::tx_rx_task;
#[cfg(target_os = "macos")]
use crate::osal::macos::tx_rx_task;
#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
use ethercrab::std::tx_rx_task;

#[cfg(not(target_os = "windows"))]
pub(crate) struct TxRxWorker {
    handle: Option<std::thread::JoinHandle<()>>,
    done: std::sync::mpsc::Receiver<()>,
}

#[cfg(not(target_os = "windows"))]
impl TxRxWorker {
    pub(crate) fn spawn(
        interface: &str,
        pdu_tx: PduTx<'static>,
        pdu_rx: PduRx<'static>,
        tuning: PumpTuning,
    ) -> Result<Self, EtherCrabLinkError> {
        let interface = interface.to_owned();
        let (ready_tx, ready_rx) =
            std::sync::mpsc::sync_channel::<Result<(), EtherCrabLinkError>>(1);
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("autd3-ethercat-txrx".to_owned())
            .spawn(move || {
                crate::osal::thread::apply_thread_tuning(
                    tuning.priority,
                    tuning.policy,
                    tuning.affinity,
                );
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_io()
                    .enable_time()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(e) => {
                        let _ = ready_tx.send(Err(EtherCrabLinkError::Io(e)));
                        return;
                    }
                };
                runtime.block_on(async move {
                    // `AsyncFd` registers with this runtime's IO driver on construction.
                    let tx_rx_fut = match tx_rx_task(&interface, pdu_tx, pdu_rx) {
                        Ok(fut) => {
                            let _ = ready_tx.send(Ok(()));
                            fut
                        }
                        Err(e) => {
                            let _ = ready_tx.send(Err(EtherCrabLinkError::Io(e)));
                            return;
                        }
                    };
                    if let Err(e) = tx_rx_fut.await {
                        tracing::error!("tx/rx task exited: {e}");
                    }
                });
                let _ = done_tx.send(());
            })
            .map_err(EtherCrabLinkError::Io)?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                handle: Some(thread),
                done: done_rx,
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                let _ = thread.join();
                Err(EtherCrabLinkError::Io(std::io::Error::other(
                    "tx/rx worker terminated before becoming ready",
                )))
            }
        }
    }

    pub(crate) fn shutdown(&mut self) -> bool {
        let stopped = self.done.recv_timeout(TX_RX_EXIT_TIMEOUT).is_ok();
        if stopped && let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        stopped
    }
}

#[cfg(target_os = "windows")]
pub(crate) struct TxRxWorker {
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
    done: std::sync::mpsc::Receiver<()>,
}

#[cfg(target_os = "windows")]
impl TxRxWorker {
    pub(crate) fn spawn(
        interface: &str,
        pdu_tx: PduTx<'static>,
        pdu_rx: PduRx<'static>,
        tuning: PumpTuning,
    ) -> Result<Self, EtherCrabLinkError> {
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let interface = interface.to_owned();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn({
            let running = std::sync::Arc::clone(&running);
            move || {
                if let Err(e) = crate::osal::windows::tx_rx_task_blocking(
                    &interface, pdu_tx, pdu_rx, &running, &ready_tx, tuning,
                ) {
                    tracing::error!("tx/rx task exited: {e}");
                }
                let _ = done_tx.send(());
            }
        });

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                running,
                handle: Some(handle),
                done: done_rx,
            }),
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(e)
            }
            Err(_) => {
                let _ = handle.join();
                Err(EtherCrabLinkError::Io(std::io::Error::other(
                    "tx/rx worker terminated before becoming ready",
                )))
            }
        }
    }

    pub(crate) fn shutdown(&mut self) -> bool {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
        if let Some(handle) = &self.handle {
            handle.thread().unpark();
        }
        let stopped = self.done.recv_timeout(TX_RX_EXIT_TIMEOUT).is_ok();
        if stopped && let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        stopped
    }
}
