use std::num::NonZeroUsize;
use std::time::Duration;

use autd3_rs_core::Interface;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportOption {
    pub iface: Interface,
    pub send_buffer_bytes: Option<NonZeroUsize>,
    pub sync_on_open: bool,
    pub heartbeat_interval: Option<Duration>,
}

impl Default for TransportOption {
    fn default() -> Self {
        Self {
            iface: Interface::Auto,
            send_buffer_bytes: NonZeroUsize::new(8 * 1024),
            sync_on_open: true,
            heartbeat_interval: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_follow_the_spec() {
        let option = TransportOption::default();
        assert_eq!(option.iface, Interface::Auto);
        assert_eq!(option.send_buffer_bytes, NonZeroUsize::new(8192));
        assert!(option.sync_on_open);
        assert_eq!(option.heartbeat_interval, None);
    }
}
