use std::num::NonZeroUsize;
use std::time::Duration;

use autd3_rs_core::Interface;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportOption {
    pub iface: Interface,
    pub open_timeout: Duration,
    pub lost_timeout: Duration,
    pub ack_timeout: Duration,
    pub send_buffer_bytes: Option<NonZeroUsize>,
    pub sync_on_open: bool,
}

impl Default for TransportOption {
    fn default() -> Self {
        Self {
            iface: Interface::Auto,
            open_timeout: Duration::from_secs(40),
            lost_timeout: Duration::from_millis(100),
            ack_timeout: Duration::from_millis(10),
            send_buffer_bytes: NonZeroUsize::new(8 * 1024),
            sync_on_open: true,
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
        assert_eq!(option.open_timeout, Duration::from_secs(40));
        assert_eq!(option.lost_timeout, Duration::from_millis(100));
        assert_eq!(option.ack_timeout, Duration::from_millis(10));
        assert_eq!(option.send_buffer_bytes, NonZeroUsize::new(8192));
        assert!(option.sync_on_open);
    }
}
