use std::sync::atomic::AtomicBool;

pub struct ControlState {
    pub mod_enabled: AtomicBool,
}

impl Default for ControlState {
    fn default() -> Self {
        Self {
            mod_enabled: AtomicBool::new(true),
        }
    }
}
