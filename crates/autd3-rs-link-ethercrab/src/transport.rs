use std::sync::Arc;

use ethercrab::{MainDevice, MainDeviceConfig, PduLoop, PduRx, PduStorage, PduTx, Timeouts};

use crate::error::EtherCrabLinkError;
use crate::osal::thread::PumpTuning;
use crate::osal::worker::TxRxWorker;

const ETHERNET_PDU_CAPACITY: usize = 1486;
const MAX_PDU_DATA: usize = PduStorage::element_size(ETHERNET_PDU_CAPACITY);
const MAX_FRAMES: usize = 32;

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
