use std::sync::{Arc, Mutex};

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
