use std::sync::{Arc, Mutex, PoisonError};

use crate::mirror::FirmwareState;

#[derive(Clone, Debug)]
pub(crate) enum Mirror {
    Synced(Vec<FirmwareState>),
    Desynced,
}

#[derive(Clone)]
pub(crate) struct MirrorHandle {
    pub(crate) state: Arc<Mutex<Mirror>>,
    pub(crate) enabled: bool,
}

impl MirrorHandle {
    pub(crate) fn set(&self, mirror: Mirror) {
        if self.enabled {
            *self.state.lock().unwrap_or_else(PoisonError::into_inner) = mirror;
        }
    }

    pub(crate) fn desync(&self) {
        self.set(Mirror::Desynced);
    }
}
