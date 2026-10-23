use core::num::NonZeroU16;
use core::time::Duration;

use zerocopy::little_endian::{U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::try_read_exact;
use crate::Error;
use crate::config::{CpuConfig, FpgaBusWait, MaxFreqPpb, PathDelayFilterShift, PtpConfig};

#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
#[error("CPU config `{field}` = {value:?} must be a multiple of {unit:?} within {min:?}..={max:?}")]
pub struct CpuConfigOutOfRange {
    pub field: &'static str,
    pub value: Duration,
    pub unit: Duration,
    pub min: Duration,
    pub max: Duration,
}

fn to_nanos(field: &'static str, value: Duration) -> Result<U32, CpuConfigOutOfRange> {
    let nanos = value.as_nanos();
    if nanos > u128::from(u32::MAX) {
        return Err(CpuConfigOutOfRange {
            field,
            value,
            unit: Duration::from_nanos(1),
            min: Duration::ZERO,
            max: Duration::from_nanos(u64::from(u32::MAX)),
        });
    }
    Ok(U32::new(nanos as u32))
}

fn to_millis_at_least(
    field: &'static str,
    value: Duration,
    min: Duration,
) -> Result<U16, CpuConfigOutOfRange> {
    let millis = value.as_millis();
    if !value.subsec_nanos().is_multiple_of(1_000_000)
        || millis < min.as_millis()
        || millis > u128::from(u16::MAX)
    {
        return Err(CpuConfigOutOfRange {
            field,
            value,
            unit: Duration::from_millis(1),
            min,
            max: Duration::from_millis(u64::from(u16::MAX)),
        });
    }
    Ok(U16::new(millis as u16))
}

fn to_millis(field: &'static str, value: Duration) -> Result<U16, CpuConfigOutOfRange> {
    to_millis_at_least(field, value, Duration::ZERO)
}

const FAILSAFE_DISABLED: u16 = 0;
const WATCHDOG_DISABLED: u16 = 0;
const PAUSE_DISABLED: u16 = 0;

fn from_nanos(value: U32) -> Duration {
    Duration::from_nanos(u64::from(value.get()))
}

fn from_millis(value: U16) -> Duration {
    Duration::from_millis(u64::from(value.get()))
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned,
)]
#[repr(C)]
pub struct SetCpuConfigPayload {
    pub sync_guard_ns: U32,
    pub ptp_sync_interval_ms: U16,
    pub ptp_tx_timestamp_timeout_ms: U16,
    pub ptp_delay_resp_timeout_ms: U16,
    pub ptp_holdover_ms: U16,
    pub ptp_lock_samples: U16,
    pub ptp_step_threshold_ns: U32,
    pub ptp_lock_threshold_ns: U32,
    pub ptp_kp_milli: U32,
    pub ptp_ki_milli: U32,
    pub ptp_max_freq_ppb: U32,
    pub failsafe_timeout_ms: U16,
    pub ptp_delay_req_syncs: U16,
    pub ptp_path_delay_filter_shift: U16,
    pub ptp_pause_quanta: U16,
    pub ptp_pause_hold_syncs: U16,
    pub ptp_pause_retry_ms: U16,
    pub ptp_unlock_failsafe_timeout_ms: U16,
    pub fpga_bus_wait: U16,
    pub cpu_watchdog_timeout_ms: U16,
}

impl SetCpuConfigPayload {
    pub fn encode(config: &CpuConfig) -> Result<Self, CpuConfigOutOfRange> {
        let ptp = &config.ptp;
        Ok(Self {
            sync_guard_ns: to_nanos("sync_guard", config.sync_guard)?,
            ptp_sync_interval_ms: to_millis("ptp.sync_interval", ptp.sync_interval)?,
            ptp_tx_timestamp_timeout_ms: to_millis(
                "ptp.tx_timestamp_timeout",
                ptp.tx_timestamp_timeout,
            )?,
            ptp_delay_resp_timeout_ms: to_millis("ptp.delay_resp_timeout", ptp.delay_resp_timeout)?,
            ptp_holdover_ms: to_millis("ptp.holdover", ptp.holdover)?,
            ptp_lock_samples: U16::new(ptp.lock_samples.get()),
            ptp_step_threshold_ns: to_nanos("ptp.step_threshold", ptp.step_threshold)?,
            ptp_lock_threshold_ns: to_nanos("ptp.lock_threshold", ptp.lock_threshold)?,
            ptp_kp_milli: U32::new(ptp.kp_milli),
            ptp_ki_milli: U32::new(ptp.ki_milli),
            ptp_max_freq_ppb: U32::new(ptp.max_freq_ppb.get()),
            failsafe_timeout_ms: match config.failsafe_timeout {
                Some(timeout) => {
                    to_millis_at_least("failsafe_timeout", timeout, Duration::from_millis(1))?
                }
                None => U16::new(FAILSAFE_DISABLED),
            },
            ptp_delay_req_syncs: U16::new(ptp.delay_req_syncs.get()),
            ptp_path_delay_filter_shift: U16::new(u16::from(ptp.path_delay_filter_shift.get())),
            ptp_pause_quanta: U16::new(match ptp.pause_quanta {
                Some(quanta) => quanta.get(),
                None => PAUSE_DISABLED,
            }),
            ptp_pause_hold_syncs: U16::new(ptp.pause_hold_syncs),
            ptp_pause_retry_ms: to_millis("ptp.pause_retry", ptp.pause_retry)?,
            ptp_unlock_failsafe_timeout_ms: match config.ptp_unlock_failsafe_timeout {
                Some(timeout) => to_millis_at_least(
                    "ptp_unlock_failsafe_timeout",
                    timeout,
                    Duration::from_millis(1),
                )?,
                None => U16::new(FAILSAFE_DISABLED),
            },
            fpga_bus_wait: U16::new(u16::from(config.fpga_bus_wait.as_u8())),
            cpu_watchdog_timeout_ms: match config.cpu_watchdog_timeout {
                Some(timeout) => {
                    to_millis_at_least("cpu_watchdog_timeout", timeout, Duration::from_millis(1))?
                }
                None => U16::new(WATCHDOG_DISABLED),
            },
        })
    }

    #[must_use]
    pub fn decode(&self) -> Option<CpuConfig> {
        let lock_samples = NonZeroU16::new(self.ptp_lock_samples.get())?;
        let delay_req_syncs = NonZeroU16::new(self.ptp_delay_req_syncs.get())?;
        let fpga_bus_wait = FpgaBusWait::from_u8(u8::try_from(self.fpga_bus_wait.get()).ok()?)?;
        let max_freq_ppb = MaxFreqPpb::new(self.ptp_max_freq_ppb.get())?;
        let path_delay_filter_shift =
            PathDelayFilterShift::new(u8::try_from(self.ptp_path_delay_filter_shift.get()).ok()?)?;
        if self.ptp_sync_interval_ms.get() == 0 {
            return None;
        }
        Some(CpuConfig {
            sync_guard: from_nanos(self.sync_guard_ns),
            failsafe_timeout: match self.failsafe_timeout_ms.get() {
                FAILSAFE_DISABLED => None,
                _ => Some(from_millis(self.failsafe_timeout_ms)),
            },
            ptp_unlock_failsafe_timeout: match self.ptp_unlock_failsafe_timeout_ms.get() {
                FAILSAFE_DISABLED => None,
                _ => Some(from_millis(self.ptp_unlock_failsafe_timeout_ms)),
            },
            fpga_bus_wait,
            cpu_watchdog_timeout: match self.cpu_watchdog_timeout_ms.get() {
                WATCHDOG_DISABLED => None,
                _ => Some(from_millis(self.cpu_watchdog_timeout_ms)),
            },
            ptp: PtpConfig {
                sync_interval: from_millis(self.ptp_sync_interval_ms),
                tx_timestamp_timeout: from_millis(self.ptp_tx_timestamp_timeout_ms),
                delay_resp_timeout: from_millis(self.ptp_delay_resp_timeout_ms),
                holdover: from_millis(self.ptp_holdover_ms),
                lock_samples,
                step_threshold: from_nanos(self.ptp_step_threshold_ns),
                lock_threshold: from_nanos(self.ptp_lock_threshold_ns),
                kp_milli: self.ptp_kp_milli.get(),
                ki_milli: self.ptp_ki_milli.get(),
                max_freq_ppb,
                delay_req_syncs,
                path_delay_filter_shift,
                pause_quanta: NonZeroU16::new(self.ptp_pause_quanta.get()),
                pause_hold_syncs: self.ptp_pause_hold_syncs.get(),
                pause_retry: from_millis(self.ptp_pause_retry_ms),
            },
        })
    }

    pub fn parse(payload: &[u8]) -> Result<CpuConfig, Error> {
        try_read_exact::<Self>(payload)?
            .decode()
            .ok_or(Error::InvalidPayload)
    }
}

assert_layout!(SetCpuConfigPayload {
    sync_guard_ns: 0,
    ptp_sync_interval_ms: 4,
    ptp_tx_timestamp_timeout_ms: 6,
    ptp_delay_resp_timeout_ms: 8,
    ptp_holdover_ms: 10,
    ptp_lock_samples: 12,
    ptp_step_threshold_ns: 14,
    ptp_lock_threshold_ns: 18,
    ptp_kp_milli: 22,
    ptp_ki_milli: 26,
    ptp_max_freq_ppb: 30,
    failsafe_timeout_ms: 34,
    ptp_delay_req_syncs: 36,
    ptp_path_delay_filter_shift: 38,
    ptp_pause_quanta: 40,
    ptp_pause_hold_syncs: 42,
    ptp_pause_retry_ms: 44,
    ptp_unlock_failsafe_timeout_ms: 46,
    fpga_bus_wait: 48,
    cpu_watchdog_timeout_ms: 50,
} size 52);

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn a_config_survives_the_round_trip() {
        let config = CpuConfig {
            sync_guard: Duration::from_micros(500),
            failsafe_timeout: Some(Duration::from_millis(24)),
            ptp_unlock_failsafe_timeout: Some(Duration::from_millis(29)),
            fpga_bus_wait: FpgaBusWait::Cycles2,
            cpu_watchdog_timeout: Some(Duration::from_millis(30)),
            ptp: PtpConfig {
                sync_interval: Duration::from_millis(14),
                tx_timestamp_timeout: Duration::from_millis(15),
                delay_resp_timeout: Duration::from_millis(16),
                holdover: Duration::from_millis(17),
                lock_samples: NonZeroU16::new(18).unwrap(),
                step_threshold: Duration::from_nanos(19),
                lock_threshold: Duration::from_nanos(20),
                kp_milli: 21,
                ki_milli: 22,
                max_freq_ppb: MaxFreqPpb::new(23).unwrap(),
                delay_req_syncs: NonZeroU16::new(25).unwrap(),
                path_delay_filter_shift: PathDelayFilterShift::new(5).unwrap(),
                pause_quanta: NonZeroU16::new(26),
                pause_hold_syncs: 27,
                pause_retry: Duration::from_millis(28),
            },
        };
        let payload = SetCpuConfigPayload::encode(&config).unwrap();
        assert_eq!(payload.sync_guard_ns.get(), 500_000);
        assert_eq!(payload.failsafe_timeout_ms.get(), 24);
        assert_eq!(payload.ptp_delay_req_syncs.get(), 25);
        assert_eq!(payload.ptp_path_delay_filter_shift.get(), 5);
        assert_eq!(payload.ptp_pause_quanta.get(), 26);
        assert_eq!(payload.ptp_pause_hold_syncs.get(), 27);
        assert_eq!(payload.ptp_pause_retry_ms.get(), 28);
        assert_eq!(payload.ptp_unlock_failsafe_timeout_ms.get(), 29);
        assert_eq!(payload.fpga_bus_wait.get(), 2);
        assert_eq!(payload.cpu_watchdog_timeout_ms.get(), 30);
        assert_eq!(payload.decode(), Some(config));
        assert_eq!(SetCpuConfigPayload::parse(payload.as_bytes()), Ok(config));
    }

    #[test]
    fn the_default_config_is_these_bytes_on_the_wire() {
        let payload = SetCpuConfigPayload::encode(&CpuConfig::default()).unwrap();
        assert_eq!(
            payload.as_bytes(),
            &[
                0x90, 0xD0, 0x03, 0x00, 0x08, 0x00, 0x03, 0x00, 0x06, 0x00, 0xE8, 0x03, 0x40, 0x00,
                0x10, 0x27, 0x00, 0x00, 0x64, 0x00, 0x00, 0x00, 0x32, 0x00, 0x00, 0x00, 0x01, 0x00,
                0x00, 0x00, 0x20, 0xA1, 0x07, 0x00, 0x00, 0x00, 0x20, 0x00, 0x05, 0x00, 0x30, 0x00,
                0x40, 0x00, 0x08, 0x00, 0x00, 0x00, 0x03, 0x00, 0xE8, 0x03,
            ]
        );
    }

    const NS_MAX: Duration = Duration::from_nanos(u32::MAX as u64);
    const MS_MIN: Duration = Duration::from_millis(1);
    const MS_MAX: Duration = Duration::from_millis(u16::MAX as u64);

    #[rstest]
    #[case(CpuConfig { sync_guard: Duration::ZERO, ..CpuConfig::default() })]
    #[case(CpuConfig { sync_guard: NS_MAX, ..CpuConfig::default() })]
    #[case(CpuConfig { failsafe_timeout: None, ..CpuConfig::default() })]
    #[case(CpuConfig { failsafe_timeout: Some(MS_MIN), ..CpuConfig::default() })]
    #[case(CpuConfig { failsafe_timeout: Some(MS_MAX), ..CpuConfig::default() })]
    #[case(CpuConfig { ptp_unlock_failsafe_timeout: None, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp_unlock_failsafe_timeout: Some(MS_MIN), ..CpuConfig::default() })]
    #[case(CpuConfig { ptp_unlock_failsafe_timeout: Some(MS_MAX), ..CpuConfig::default() })]
    #[case(CpuConfig { fpga_bus_wait: FpgaBusWait::Cycles2, ..CpuConfig::default() })]
    #[case(CpuConfig { fpga_bus_wait: FpgaBusWait::Cycles3, ..CpuConfig::default() })]
    #[case(CpuConfig { cpu_watchdog_timeout: None, ..CpuConfig::default() })]
    #[case(CpuConfig { cpu_watchdog_timeout: Some(MS_MIN), ..CpuConfig::default() })]
    #[case(CpuConfig { cpu_watchdog_timeout: Some(MS_MAX), ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { sync_interval: MS_MIN, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { sync_interval: MS_MAX, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { tx_timestamp_timeout: Duration::ZERO, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { tx_timestamp_timeout: MS_MAX, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { delay_resp_timeout: Duration::ZERO, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { delay_resp_timeout: MS_MAX, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { holdover: Duration::ZERO, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { holdover: MS_MAX, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { lock_samples: NonZeroU16::MIN, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { lock_samples: NonZeroU16::MAX, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { step_threshold: Duration::ZERO, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { step_threshold: NS_MAX, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { lock_threshold: Duration::ZERO, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { lock_threshold: NS_MAX, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { kp_milli: 0, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { kp_milli: u32::MAX, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { ki_milli: 0, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { ki_milli: u32::MAX, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { max_freq_ppb: MaxFreqPpb::MIN, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { max_freq_ppb: MaxFreqPpb::MAX, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { delay_req_syncs: NonZeroU16::MIN, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { delay_req_syncs: NonZeroU16::MAX, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { path_delay_filter_shift: PathDelayFilterShift::MIN, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { path_delay_filter_shift: PathDelayFilterShift::MAX, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { pause_quanta: None, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { pause_quanta: Some(NonZeroU16::MIN), ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { pause_quanta: Some(NonZeroU16::MAX), ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { pause_hold_syncs: 0, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { pause_hold_syncs: u16::MAX, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { pause_retry: Duration::ZERO, ..PtpConfig::default() }, ..CpuConfig::default() })]
    #[case(CpuConfig { ptp: PtpConfig { pause_retry: MS_MAX, ..PtpConfig::default() }, ..CpuConfig::default() })]
    fn a_config_at_the_edge_of_what_encode_accepts_decodes_to_itself(#[case] config: CpuConfig) {
        let payload = SetCpuConfigPayload::encode(&config).unwrap();
        assert_eq!(payload.decode(), Some(config));
        assert_eq!(SetCpuConfigPayload::parse(payload.as_bytes()), Ok(config));
    }

    #[rstest]
    #[case(CpuConfig { ptp: PtpConfig { sync_interval: Duration::ZERO, ..PtpConfig::default() }, ..CpuConfig::default() })]
    fn a_zero_duration_the_firmware_refuses_is_left_to_the_device(#[case] config: CpuConfig) {
        let payload = SetCpuConfigPayload::encode(&config).unwrap();
        assert_eq!(payload.decode(), None);
    }

    #[test]
    fn the_bounded_ptp_fields_reach_the_wire_at_both_ends() {
        let payload = SetCpuConfigPayload::encode(&CpuConfig {
            ptp: PtpConfig {
                max_freq_ppb: MaxFreqPpb::MAX,
                path_delay_filter_shift: PathDelayFilterShift::MAX,
                ..PtpConfig::default()
            },
            ..CpuConfig::default()
        })
        .unwrap();
        assert_eq!(payload.ptp_max_freq_ppb.get(), 0x7FFF_FFFF);
        assert_eq!(payload.ptp_path_delay_filter_shift.get(), 16);
    }

    #[rstest]
    #[case(17)]
    #[case(0x0100)]
    #[case(0x0105)]
    #[case(u16::MAX)]
    fn a_path_delay_filter_shift_past_sixteen_does_not_decode(#[case] shift: u16) {
        let mut payload = SetCpuConfigPayload::encode(&CpuConfig::default()).unwrap();
        payload.ptp_path_delay_filter_shift = U16::new(shift);
        assert_eq!(payload.decode(), None);
    }

    #[test]
    fn the_failsafe_is_disabled_by_default_and_zero_on_the_wire() {
        let default = CpuConfig::default();
        assert_eq!(default.failsafe_timeout, None);
        let payload = SetCpuConfigPayload::encode(&default).unwrap();
        assert_eq!(payload.failsafe_timeout_ms.get(), 0);
        assert_eq!(payload.decode(), Some(default));

        let enabled = SetCpuConfigPayload::encode(&CpuConfig {
            failsafe_timeout: Some(Duration::from_millis(500)),
            ..CpuConfig::default()
        })
        .unwrap();
        assert_eq!(enabled.failsafe_timeout_ms.get(), 500);
    }

    #[test]
    fn the_ptp_unlock_failsafe_is_disabled_by_default_and_zero_on_the_wire() {
        let default = CpuConfig::default();
        assert_eq!(default.ptp_unlock_failsafe_timeout, None);
        let payload = SetCpuConfigPayload::encode(&default).unwrap();
        assert_eq!(payload.ptp_unlock_failsafe_timeout_ms.get(), 0);
        assert_eq!(payload.decode(), Some(default));
    }

    #[rstest]
    #[case(Duration::ZERO)]
    #[case(Duration::from_micros(1500))]
    #[case(Duration::from_millis(65536))]
    fn a_ptp_unlock_failsafe_timeout_the_wire_would_read_as_disabled_is_rejected(
        #[case] timeout: Duration,
    ) {
        assert_eq!(
            SetCpuConfigPayload::encode(&CpuConfig {
                ptp_unlock_failsafe_timeout: Some(timeout),
                ..CpuConfig::default()
            }),
            Err(CpuConfigOutOfRange {
                field: "ptp_unlock_failsafe_timeout",
                value: timeout,
                unit: Duration::from_millis(1),
                min: Duration::from_millis(1),
                max: Duration::from_millis(65535),
            })
        );
    }

    #[rstest]
    #[case(Duration::from_millis(1))]
    #[case(Duration::from_millis(65535))]
    fn a_ptp_unlock_failsafe_timeout_at_the_wire_limits_survives_the_round_trip(
        #[case] timeout: Duration,
    ) {
        let config = CpuConfig {
            ptp_unlock_failsafe_timeout: Some(timeout),
            ..CpuConfig::default()
        };
        assert_eq!(
            SetCpuConfigPayload::encode(&config).unwrap().decode(),
            Some(config)
        );
    }

    #[test]
    fn the_cpu_watchdog_is_one_second_by_default_and_zero_on_the_wire_when_disabled() {
        let default = SetCpuConfigPayload::encode(&CpuConfig::default()).unwrap();
        assert_eq!(default.cpu_watchdog_timeout_ms.get(), 1000);

        let config = CpuConfig {
            cpu_watchdog_timeout: None,
            ..CpuConfig::default()
        };
        let payload = SetCpuConfigPayload::encode(&config).unwrap();
        assert_eq!(payload.cpu_watchdog_timeout_ms.get(), 0);
        assert_eq!(payload.decode(), Some(config));
    }

    #[rstest]
    #[case(Duration::ZERO)]
    #[case(Duration::from_micros(1500))]
    #[case(Duration::from_millis(65536))]
    fn a_cpu_watchdog_timeout_the_wire_would_read_as_disabled_is_rejected(
        #[case] timeout: Duration,
    ) {
        assert_eq!(
            SetCpuConfigPayload::encode(&CpuConfig {
                cpu_watchdog_timeout: Some(timeout),
                ..CpuConfig::default()
            }),
            Err(CpuConfigOutOfRange {
                field: "cpu_watchdog_timeout",
                value: timeout,
                unit: Duration::from_millis(1),
                min: Duration::from_millis(1),
                max: Duration::from_millis(65535),
            })
        );
    }

    #[test]
    fn the_fpga_bus_wait_is_three_cycles_by_default() {
        let default = CpuConfig::default();
        assert_eq!(default.fpga_bus_wait, FpgaBusWait::Cycles3);
        let payload = SetCpuConfigPayload::encode(&default).unwrap();
        assert_eq!(payload.fpga_bus_wait.get(), 3);
    }

    #[rstest]
    #[case(0)]
    #[case(1)]
    #[case(4)]
    #[case(0x0102)]
    #[case(0x0103)]
    fn an_fpga_bus_wait_the_fpga_cannot_capture_is_rejected(#[case] cycles: u16) {
        let mut payload = SetCpuConfigPayload::encode(&CpuConfig::default()).unwrap();
        payload.fpga_bus_wait = U16::new(cycles);
        assert_eq!(payload.decode(), None);
        assert_eq!(
            SetCpuConfigPayload::parse(payload.as_bytes()),
            Err(Error::InvalidPayload)
        );
    }

    #[test]
    fn a_disabled_pause_is_zero_on_the_wire() {
        let config = CpuConfig {
            ptp: PtpConfig {
                pause_quanta: None,
                ..PtpConfig::default()
            },
            ..CpuConfig::default()
        };
        let payload = SetCpuConfigPayload::encode(&config).unwrap();
        assert_eq!(payload.ptp_pause_quanta.get(), 0);
        assert_eq!(payload.decode(), Some(config));

        let default = SetCpuConfigPayload::encode(&CpuConfig::default()).unwrap();
        assert_eq!(default.ptp_delay_req_syncs.get(), 32);
        assert_eq!(default.ptp_path_delay_filter_shift.get(), 5);
        assert_eq!(default.ptp_pause_quanta.get(), 48);
        assert_eq!(default.ptp_pause_hold_syncs.get(), 64);
        assert_eq!(default.ptp_pause_retry_ms.get(), 8);
    }

    #[rstest]
    #[case(Duration::ZERO)]
    #[case(Duration::from_micros(500))]
    #[case(Duration::from_micros(1500))]
    #[case(Duration::from_millis(65536))]
    fn a_failsafe_timeout_the_wire_would_read_as_disabled_is_rejected(#[case] timeout: Duration) {
        assert_eq!(
            SetCpuConfigPayload::encode(&CpuConfig {
                failsafe_timeout: Some(timeout),
                ..CpuConfig::default()
            }),
            Err(CpuConfigOutOfRange {
                field: "failsafe_timeout",
                value: timeout,
                unit: Duration::from_millis(1),
                min: Duration::from_millis(1),
                max: Duration::from_millis(65535),
            })
        );
    }

    #[rstest]
    #[case(Duration::from_millis(1))]
    #[case(Duration::from_millis(65535))]
    fn a_failsafe_timeout_at_the_wire_limits_survives_the_round_trip(#[case] timeout: Duration) {
        let config = CpuConfig {
            failsafe_timeout: Some(timeout),
            ..CpuConfig::default()
        };
        assert_eq!(
            SetCpuConfigPayload::encode(&config).unwrap().decode(),
            Some(config)
        );
    }

    #[test]
    fn durations_the_wire_cannot_carry_are_rejected() {
        let rejected = [
            (
                "sync_guard",
                CpuConfig {
                    sync_guard: Duration::from_secs(5),
                    ..CpuConfig::default()
                },
            ),
            (
                "ptp.pause_retry",
                CpuConfig {
                    ptp: PtpConfig {
                        pause_retry: Duration::from_micros(1500),
                        ..PtpConfig::default()
                    },
                    ..CpuConfig::default()
                },
            ),
            (
                "ptp.holdover",
                CpuConfig {
                    ptp: PtpConfig {
                        holdover: Duration::from_secs(66),
                        ..PtpConfig::default()
                    },
                    ..CpuConfig::default()
                },
            ),
        ];
        for (field, config) in rejected {
            assert_eq!(
                SetCpuConfigPayload::encode(&config).map_err(|e| e.field),
                Err(field)
            );
        }
    }

    #[test]
    fn values_the_firmware_cannot_run_with_do_not_decode() {
        let valid = SetCpuConfigPayload::encode(&CpuConfig::default()).unwrap();
        let rejected = [
            SetCpuConfigPayload {
                ptp_sync_interval_ms: U16::new(0),
                ..valid
            },
            SetCpuConfigPayload {
                ptp_lock_samples: U16::new(0),
                ..valid
            },
            SetCpuConfigPayload {
                ptp_max_freq_ppb: U32::new(MaxFreqPpb::MAX.get() + 1),
                ..valid
            },
            SetCpuConfigPayload {
                ptp_delay_req_syncs: U16::new(0),
                ..valid
            },
            SetCpuConfigPayload {
                ptp_path_delay_filter_shift: U16::new(17),
                ..valid
            },
        ];
        for payload in rejected {
            assert_eq!(payload.decode(), None);
            assert_eq!(
                SetCpuConfigPayload::parse(payload.as_bytes()),
                Err(Error::InvalidPayload)
            );
        }
        assert_eq!(
            SetCpuConfigPayload::parse(&valid.as_bytes()[1..]),
            Err(Error::InvalidPayload)
        );
    }
}
