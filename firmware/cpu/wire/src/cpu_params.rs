use core::num::{NonZeroU16, NonZeroU32};
use core::time::Duration;

pub const SYS_TIME_TRANSITION_MARGIN: Duration = Duration::from_millis(10);
pub const FPGA_WAIT_UPDATE_MAX_POLLS: NonZeroU32 = NonZeroU32::new(1_000_000).unwrap();
pub const FPGA_FLASH_MAX_POLLS: NonZeroU32 = NonZeroU32::new(2_000_000_000).unwrap();
pub const SYNC_GUARD: Duration = Duration::from_micros(250);
pub const UPDATE_ACTIVATE_DELAY: Duration = Duration::from_millis(100);
pub const FAILSAFE_TIMEOUT: Duration = Duration::from_millis(500);

pub const PTP_SYNC_INTERVAL: Duration = Duration::from_millis(16);
pub const PTP_TX_TIMESTAMP_TIMEOUT: Duration = Duration::from_millis(3);
pub const PTP_DELAY_RESP_TIMEOUT: Duration = Duration::from_millis(6);
pub const PTP_HOLDOVER: Duration = Duration::from_secs(1);
pub const PTP_DELAY_REQ_SYNCS: NonZeroU16 = NonZeroU16::new(16).unwrap();
pub const PTP_PATH_DELAY_FILTER_SHIFT: u8 = 3;
pub const PTP_PATH_DELAY_FILTER_SHIFT_MAX: u8 = 16;
pub const PTP_PAUSE_QUANTA: NonZeroU16 = NonZeroU16::new(48).unwrap();
pub const PTP_PAUSE_HOLD_SYNCS: u16 = 64;
pub const PTP_PAUSE_RETRY: Duration = Duration::from_millis(8);
pub const PTP_LOCK_SAMPLES: NonZeroU16 = NonZeroU16::new(64).unwrap();
pub const PTP_STEP_THRESHOLD: Duration = Duration::from_micros(10);
pub const PTP_LOCK_THRESHOLD: Duration = Duration::from_nanos(100);
pub const PTP_KP_MILLI: u32 = 100;
pub const PTP_KI_MILLI: u32 = 20;
pub const PTP_MAX_FREQ_PPB: u32 = 500_000;
pub const PTP_MAX_FREQ_PPB_MAX: u32 = i32::MAX.unsigned_abs();
