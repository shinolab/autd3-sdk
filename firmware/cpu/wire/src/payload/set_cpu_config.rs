use core::num::{NonZeroU16, NonZeroU32};
use core::time::Duration;

use zerocopy::little_endian::{U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::try_read_exact;
use crate::Error;
use crate::config::{CpuConfig, FpgaBusWait, PtpConfig};
use crate::cpu_params;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CpuConfigOutOfRange {
    pub field: &'static str,
    pub value: Duration,
    pub unit: Duration,
    pub min: Duration,
    pub max: Duration,
}

const fn to_nanos(field: &'static str, value: Duration) -> Result<U32, CpuConfigOutOfRange> {
    let nanos = value.as_nanos();
    if nanos > u32::MAX as u128 {
        return Err(CpuConfigOutOfRange {
            field,
            value,
            unit: Duration::from_nanos(1),
            min: Duration::ZERO,
            max: Duration::from_nanos(u32::MAX as u64),
        });
    }
    Ok(U32::new(nanos as u32))
}

const fn to_millis_at_least(
    field: &'static str,
    value: Duration,
    min: Duration,
) -> Result<U16, CpuConfigOutOfRange> {
    let millis = value.as_millis();
    if !value.subsec_nanos().is_multiple_of(1_000_000)
        || millis < min.as_millis()
        || millis > u16::MAX as u128
    {
        return Err(CpuConfigOutOfRange {
            field,
            value,
            unit: Duration::from_millis(1),
            min,
            max: Duration::from_millis(u16::MAX as u64),
        });
    }
    Ok(U16::new(millis as u16))
}

const fn to_millis(field: &'static str, value: Duration) -> Result<U16, CpuConfigOutOfRange> {
    to_millis_at_least(field, value, Duration::ZERO)
}

const FAILSAFE_DISABLED: u16 = 0;
const PAUSE_DISABLED: u16 = 0;

const fn from_nanos(value: U32) -> Duration {
    Duration::from_nanos(value.get() as u64)
}

const fn from_millis(value: U16) -> Duration {
    Duration::from_millis(value.get() as u64)
}

macro_rules! tri {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => return Err(e),
        }
    };
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned,
)]
#[repr(C)]
pub struct SetCpuConfigPayload {
    pub sys_time_transition_margin_ns: U32,
    pub fpga_wait_update_max_polls: U32,
    pub fpga_flash_max_polls: U32,
    pub sync_guard_ns: U32,
    pub update_activate_delay_ms: U16,
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
}

impl SetCpuConfigPayload {
    pub const fn encode(config: &CpuConfig) -> Result<Self, CpuConfigOutOfRange> {
        let ptp = &config.ptp;
        Ok(Self {
            sys_time_transition_margin_ns: tri!(to_nanos(
                "sys_time_transition_margin",
                config.sys_time_transition_margin
            )),
            fpga_wait_update_max_polls: U32::new(config.fpga_wait_update_max_polls.get()),
            fpga_flash_max_polls: U32::new(config.fpga_flash_max_polls.get()),
            sync_guard_ns: tri!(to_nanos("sync_guard", config.sync_guard)),
            update_activate_delay_ms: tri!(to_millis(
                "update_activate_delay",
                config.update_activate_delay
            )),
            ptp_sync_interval_ms: tri!(to_millis("ptp.sync_interval", ptp.sync_interval)),
            ptp_tx_timestamp_timeout_ms: tri!(to_millis(
                "ptp.tx_timestamp_timeout",
                ptp.tx_timestamp_timeout
            )),
            ptp_delay_resp_timeout_ms: tri!(to_millis(
                "ptp.delay_resp_timeout",
                ptp.delay_resp_timeout
            )),
            ptp_holdover_ms: tri!(to_millis("ptp.holdover", ptp.holdover)),
            ptp_lock_samples: U16::new(ptp.lock_samples.get()),
            ptp_step_threshold_ns: tri!(to_nanos("ptp.step_threshold", ptp.step_threshold)),
            ptp_lock_threshold_ns: tri!(to_nanos("ptp.lock_threshold", ptp.lock_threshold)),
            ptp_kp_milli: U32::new(ptp.kp_milli),
            ptp_ki_milli: U32::new(ptp.ki_milli),
            ptp_max_freq_ppb: U32::new(ptp.max_freq_ppb),
            failsafe_timeout_ms: match config.failsafe_timeout {
                Some(timeout) => tri!(to_millis_at_least(
                    "failsafe_timeout",
                    timeout,
                    Duration::from_millis(1)
                )),
                None => U16::new(FAILSAFE_DISABLED),
            },
            ptp_delay_req_syncs: U16::new(ptp.delay_req_syncs.get()),
            ptp_path_delay_filter_shift: U16::new(ptp.path_delay_filter_shift as u16),
            ptp_pause_quanta: U16::new(match ptp.pause_quanta {
                Some(quanta) => quanta.get(),
                None => PAUSE_DISABLED,
            }),
            ptp_pause_hold_syncs: U16::new(ptp.pause_hold_syncs),
            ptp_pause_retry_ms: tri!(to_millis("ptp.pause_retry", ptp.pause_retry)),
            ptp_unlock_failsafe_timeout_ms: match config.ptp_unlock_failsafe_timeout {
                Some(timeout) => tri!(to_millis_at_least(
                    "ptp_unlock_failsafe_timeout",
                    timeout,
                    Duration::from_millis(1)
                )),
                None => U16::new(FAILSAFE_DISABLED),
            },
            fpga_bus_wait: U16::new(config.fpga_bus_wait.as_u8() as u16),
        })
    }

    #[must_use]
    pub const fn decode(&self) -> Option<CpuConfig> {
        let (
            Some(fpga_wait_update_max_polls),
            Some(fpga_flash_max_polls),
            Some(lock_samples),
            Some(delay_req_syncs),
        ) = (
            NonZeroU32::new(self.fpga_wait_update_max_polls.get()),
            NonZeroU32::new(self.fpga_flash_max_polls.get()),
            NonZeroU16::new(self.ptp_lock_samples.get()),
            NonZeroU16::new(self.ptp_delay_req_syncs.get()),
        )
        else {
            return None;
        };
        let fpga_bus_wait = self.fpga_bus_wait.get();
        let Some(fpga_bus_wait) = (if fpga_bus_wait > u8::MAX as u16 {
            None
        } else {
            FpgaBusWait::from_u8(fpga_bus_wait as u8)
        }) else {
            return None;
        };
        if self.update_activate_delay_ms.get() == 0
            || self.ptp_sync_interval_ms.get() == 0
            || self.ptp_max_freq_ppb.get() > cpu_params::PTP_MAX_FREQ_PPB_MAX
            || self.ptp_path_delay_filter_shift.get()
                > cpu_params::PTP_PATH_DELAY_FILTER_SHIFT_MAX as u16
        {
            return None;
        }
        Some(CpuConfig {
            sys_time_transition_margin: from_nanos(self.sys_time_transition_margin_ns),
            fpga_wait_update_max_polls,
            fpga_flash_max_polls,
            sync_guard: from_nanos(self.sync_guard_ns),
            update_activate_delay: from_millis(self.update_activate_delay_ms),
            failsafe_timeout: match self.failsafe_timeout_ms.get() {
                FAILSAFE_DISABLED => None,
                _ => Some(from_millis(self.failsafe_timeout_ms)),
            },
            ptp_unlock_failsafe_timeout: match self.ptp_unlock_failsafe_timeout_ms.get() {
                FAILSAFE_DISABLED => None,
                _ => Some(from_millis(self.ptp_unlock_failsafe_timeout_ms)),
            },
            fpga_bus_wait,
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
                max_freq_ppb: self.ptp_max_freq_ppb.get(),
                delay_req_syncs,
                path_delay_filter_shift: self.ptp_path_delay_filter_shift.get() as u8,
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

const _: () =
    assert!(core::mem::offset_of!(SetCpuConfigPayload, sys_time_transition_margin_ns) == 0);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, fpga_wait_update_max_polls) == 4);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, fpga_flash_max_polls) == 8);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, sync_guard_ns) == 12);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, update_activate_delay_ms) == 16);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, ptp_sync_interval_ms) == 18);
const _: () =
    assert!(core::mem::offset_of!(SetCpuConfigPayload, ptp_tx_timestamp_timeout_ms) == 20);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, ptp_delay_resp_timeout_ms) == 22);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, ptp_holdover_ms) == 24);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, ptp_lock_samples) == 26);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, ptp_step_threshold_ns) == 28);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, ptp_lock_threshold_ns) == 32);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, ptp_kp_milli) == 36);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, ptp_ki_milli) == 40);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, ptp_max_freq_ppb) == 44);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, failsafe_timeout_ms) == 48);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, ptp_delay_req_syncs) == 50);
const _: () =
    assert!(core::mem::offset_of!(SetCpuConfigPayload, ptp_path_delay_filter_shift) == 52);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, ptp_pause_quanta) == 54);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, ptp_pause_hold_syncs) == 56);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, ptp_pause_retry_ms) == 58);
const _: () =
    assert!(core::mem::offset_of!(SetCpuConfigPayload, ptp_unlock_failsafe_timeout_ms) == 60);
const _: () = assert!(core::mem::offset_of!(SetCpuConfigPayload, fpga_bus_wait) == 62);
const _: () = assert!(core::mem::size_of::<SetCpuConfigPayload>() == 64);

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn a_config_survives_the_round_trip() {
        let config = CpuConfig {
            sys_time_transition_margin: Duration::ZERO,
            fpga_wait_update_max_polls: NonZeroU32::new(11).unwrap(),
            fpga_flash_max_polls: NonZeroU32::new(12).unwrap(),
            sync_guard: Duration::from_micros(500),
            update_activate_delay: Duration::from_millis(13),
            failsafe_timeout: Some(Duration::from_millis(24)),
            ptp_unlock_failsafe_timeout: Some(Duration::from_millis(29)),
            fpga_bus_wait: FpgaBusWait::Cycles2,
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
                max_freq_ppb: 23,
                delay_req_syncs: NonZeroU16::new(25).unwrap(),
                path_delay_filter_shift: 5,
                pause_quanta: NonZeroU16::new(26),
                pause_hold_syncs: 27,
                pause_retry: Duration::from_millis(28),
            },
        };
        let payload = SetCpuConfigPayload::encode(&config).unwrap();
        assert_eq!(payload.sync_guard_ns.get(), 500_000);
        assert_eq!(payload.update_activate_delay_ms.get(), 13);
        assert_eq!(payload.failsafe_timeout_ms.get(), 24);
        assert_eq!(payload.ptp_delay_req_syncs.get(), 25);
        assert_eq!(payload.ptp_path_delay_filter_shift.get(), 5);
        assert_eq!(payload.ptp_pause_quanta.get(), 26);
        assert_eq!(payload.ptp_pause_hold_syncs.get(), 27);
        assert_eq!(payload.ptp_pause_retry_ms.get(), 28);
        assert_eq!(payload.ptp_unlock_failsafe_timeout_ms.get(), 29);
        assert_eq!(payload.fpga_bus_wait.get(), 2);
        assert_eq!(payload.decode(), Some(config));
        assert_eq!(SetCpuConfigPayload::parse(payload.as_bytes()), Ok(config));
    }

    #[test]
    fn a_disabled_failsafe_is_zero_on_the_wire() {
        let config = CpuConfig {
            failsafe_timeout: None,
            ..CpuConfig::default()
        };
        let payload = SetCpuConfigPayload::encode(&config).unwrap();
        assert_eq!(payload.failsafe_timeout_ms.get(), 0);
        assert_eq!(payload.decode(), Some(config));

        let default = SetCpuConfigPayload::encode(&CpuConfig::default()).unwrap();
        assert_eq!(default.failsafe_timeout_ms.get(), 500);
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
                "sys_time_transition_margin",
                CpuConfig {
                    sys_time_transition_margin: Duration::from_secs(5),
                    ..CpuConfig::default()
                },
            ),
            (
                "update_activate_delay",
                CpuConfig {
                    update_activate_delay: Duration::from_micros(1500),
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
                fpga_wait_update_max_polls: U32::new(0),
                ..valid
            },
            SetCpuConfigPayload {
                fpga_flash_max_polls: U32::new(0),
                ..valid
            },
            SetCpuConfigPayload {
                update_activate_delay_ms: U16::new(0),
                ..valid
            },
            SetCpuConfigPayload {
                ptp_sync_interval_ms: U16::new(0),
                ..valid
            },
            SetCpuConfigPayload {
                ptp_lock_samples: U16::new(0),
                ..valid
            },
            SetCpuConfigPayload {
                ptp_max_freq_ppb: U32::new(cpu_params::PTP_MAX_FREQ_PPB_MAX + 1),
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
