use std::sync::Arc;

use ethercrab::{MainDevice, MainDeviceConfig, PduLoop, PduRx, PduStorage, PduTx, Timeouts};

use crate::error::EtherCrabLinkError;
use crate::rt::{CoreId, RtSchedulePolicy, ThreadPriority};

const ETHERNET_PDU_CAPACITY: usize = 1486;
const MAX_PDU_DATA: usize = PduStorage::element_size(ETHERNET_PDU_CAPACITY);
const MAX_FRAMES: usize = 32;

const TX_RX_EXIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

#[derive(Clone, Copy, Default)]
pub(crate) struct PumpTuning {
    pub(crate) priority: Option<ThreadPriority>,
    pub(crate) policy: RtSchedulePolicy,
    pub(crate) affinity: Option<CoreId>,
}

struct PduStorageWrapper {
    ptr: *mut PduStorage<MAX_FRAMES, MAX_PDU_DATA>,
}

// SAFETY: the wrapper uniquely owns the allocation and `PduStorage` is `Sync`
// (it is designed to be shared by reference across threads); the raw pointer
// only exists to decouple the allocation's lifetime from the wrapper borrow.
unsafe impl Send for PduStorageWrapper {}

impl PduStorageWrapper {
    fn new() -> Self {
        Self {
            ptr: Box::into_raw(Box::new(PduStorage::new())),
        }
    }

    #[allow(clippy::result_unit_err)]
    fn try_split(&self) -> Result<(PduTx<'static>, PduRx<'static>, PduLoop<'static>), ()> {
        // SAFETY: `ptr` comes from `Box::into_raw` in `new` and is freed only
        // in `Drop`, so it is valid here; the `'static` borrows it hands out
        // are kept alive by [`Transport`]'s teardown ordering.
        unsafe { (*self.ptr).try_split() }
    }
}

impl Drop for PduStorageWrapper {
    fn drop(&mut self) {
        // SAFETY: `ptr` comes from `Box::into_raw` in `new` and is freed
        // exactly once, here.
        drop(unsafe { Box::from_raw(self.ptr) });
    }
}

#[cfg(not(target_os = "windows"))]
struct TxRxWorker {
    handle: Option<std::thread::JoinHandle<()>>,
    done: std::sync::mpsc::Receiver<()>,
}

#[cfg(target_os = "linux")]
use crate::linux::tx_rx_task;
#[cfg(target_os = "macos")]
use crate::macos::tx_rx_task;
#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
use ethercrab::std::tx_rx_task;

#[cfg(not(target_os = "windows"))]
impl TxRxWorker {
    fn spawn(
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
                crate::rt::apply_thread_tuning(tuning.priority, tuning.policy, tuning.affinity);
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

    fn shutdown(&mut self) -> bool {
        let stopped = self.done.recv_timeout(TX_RX_EXIT_TIMEOUT).is_ok();
        if stopped && let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        stopped
    }
}

#[cfg(target_os = "windows")]
struct TxRxWorker {
    running: Arc<std::sync::atomic::AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
    done: std::sync::mpsc::Receiver<()>,
}

#[cfg(target_os = "windows")]
impl TxRxWorker {
    fn spawn(
        interface: &str,
        pdu_tx: PduTx<'static>,
        pdu_rx: PduRx<'static>,
        tuning: PumpTuning,
    ) -> Result<Self, EtherCrabLinkError> {
        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let interface = interface.to_owned();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn({
            let running = Arc::clone(&running);
            move || {
                if let Err(e) = crate::windows::tx_rx_task_blocking(
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

    fn shutdown(&mut self) -> bool {
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

pub(crate) struct Transport {
    maindevice: Option<Arc<MainDevice<'static>>>,
    worker: TxRxWorker,
    pdu_storage: Option<PduStorageWrapper>,
}

impl Transport {
    pub(crate) fn open(
        interface: &str,
        timeouts: Timeouts,
        config: MainDeviceConfig,
        tuning: PumpTuning,
    ) -> Result<Self, EtherCrabLinkError> {
        let pdu_storage = PduStorageWrapper::new();
        let (pdu_tx, pdu_rx, pdu_loop) = pdu_storage
            .try_split()
            .map_err(|()| EtherCrabLinkError::PduStorage)?;
        let maindevice = MainDevice::new(pdu_loop, timeouts, config);

        let worker = TxRxWorker::spawn(interface, pdu_tx, pdu_rx, tuning)?;

        Ok(Self {
            maindevice: Some(Arc::new(maindevice)),
            worker,
            pdu_storage: Some(pdu_storage),
        })
    }

    pub(crate) fn maindevice(&self) -> &MainDevice<'static> {
        self.maindevice.as_ref().expect("taken only in Drop")
    }

    pub(crate) fn maindevice_arc(&self) -> Arc<MainDevice<'static>> {
        Arc::clone(self.maindevice.as_ref().expect("taken only in Drop"))
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        let Some(maindevice) = self.maindevice.take() else {
            return;
        };
        let released = if let Ok(maindevice) = Arc::try_unwrap(maindevice) {
            // SAFETY: the owner has stopped cycling, so no PDUs are in flight,
            // and the groups created from this MainDevice are never used again.
            // `release_all` also signals the tx/rx task to exit.
            let _ = unsafe { maindevice.release_all() };
            true
        } else {
            tracing::warn!("maindevice still referenced at teardown; forcing tx/rx shutdown");
            false
        };

        let stopped = self.worker.shutdown();
        if !stopped || !released {
            std::mem::forget(self.pdu_storage.take());
        }
    }
}
