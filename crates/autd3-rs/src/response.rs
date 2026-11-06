use autd3_cpu_wire::udp::Flags;
use autd3_rs_core::value::SysTime;
use autd3_rs_core::{DeviceState, MAX_DEVICES};

use crate::device_set::DeviceSet;
use crate::device_status::DeviceStatus;
use crate::error::Error;
use crate::udp::DeviceReply;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    error_codes: [u8; MAX_DEVICES],
    sys_times: [u64; MAX_DEVICES],
    flags: [Flags; MAX_DEVICES],
    num_devices: usize,
    reply_data: Vec<Vec<u8>>,
    replied_devices: DeviceSet,
}

impl Default for Response {
    fn default() -> Self {
        Self {
            error_codes: [0; MAX_DEVICES],
            sys_times: [0; MAX_DEVICES],
            flags: [Flags::empty(); MAX_DEVICES],
            num_devices: 0,
            reply_data: Vec::new(),
            replied_devices: DeviceSet::empty(),
        }
    }
}

impl Response {
    #[must_use]
    pub fn from_error_codes(error_codes: &[u8]) -> Self {
        let num_devices = error_codes.len().min(MAX_DEVICES);
        let mut buf = [0u8; MAX_DEVICES];
        buf[..num_devices].copy_from_slice(&error_codes[..num_devices]);
        Self {
            error_codes: buf,
            sys_times: [0; MAX_DEVICES],
            flags: [Flags::empty(); MAX_DEVICES],
            num_devices,
            reply_data: Vec::new(),
            replied_devices: DeviceSet::first_n(num_devices),
        }
    }

    #[must_use]
    pub(crate) fn for_devices(num_devices: usize) -> Self {
        Self {
            num_devices: num_devices.min(MAX_DEVICES),
            ..Self::default()
        }
    }

    pub(crate) fn record_reply(&mut self, reply: DeviceReply) {
        let device = reply.device;
        self.error_codes[device] = reply.status;
        self.sys_times[device] = reply.sys_time;
        self.flags[device] = reply.flags;
        self.replied_devices.insert(device);
        if !reply.data.is_empty() {
            self.reply_data.resize_with(self.num_devices, Vec::new);
            self.reply_data[device] = reply.data;
        }
    }

    #[must_use]
    pub(crate) const fn replied_devices(&self) -> DeviceSet {
        self.replied_devices
    }

    #[must_use]
    pub(crate) fn has_all_replies(&self) -> bool {
        DeviceSet::first_n(self.num_devices).is_subset(&self.replied_devices)
    }

    pub(crate) fn parse_each_device<T>(
        &self,
        parse: impl Fn(&[u8]) -> Option<T>,
    ) -> Result<Vec<T>, Error> {
        (0..self.num_devices)
            .map(|device| {
                parse(self.reply_data_of(device)).ok_or(Error::UnexpectedReply { device })
            })
            .collect()
    }

    #[must_use]
    pub fn error_codes(&self) -> &[u8] {
        &self.error_codes[..self.num_devices]
    }

    #[must_use]
    pub fn reply_data(&self) -> &[Vec<u8>] {
        &self.reply_data
    }

    #[must_use]
    pub fn reply_data_of(&self, device: usize) -> &[u8] {
        self.reply_data.get(device).map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn sys_time_of(&self, device: usize) -> Option<SysTime> {
        let replied = device < self.num_devices && self.replied_devices[device];
        replied
            .then(|| self.sys_times[device])
            .filter(|&sys_time| sys_time != 0)
            .map(SysTime::from_nanos)
    }

    #[must_use]
    pub fn device_status(&self) -> Vec<DeviceStatus> {
        self.flags[..self.num_devices]
            .iter()
            .map(|&flags| DeviceStatus {
                state: if flags.contains(Flags::PTP_LOCKED | Flags::SYNC_READY) {
                    DeviceState::Ready
                } else {
                    DeviceState::Syncing
                },
                ptp_locked: flags.contains(Flags::PTP_LOCKED),
                failsafe_active: flags.contains(Flags::FAILSAFE),
            })
            .collect()
    }

    pub fn check(&self) -> Result<(), Error> {
        match self
            .error_codes()
            .iter()
            .enumerate()
            .find(|&(_, &code)| code != 0)
        {
            None => Ok(()),
            Some((device, &code)) => Err(Error::DeviceError { device, code }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(device: usize, status: u8, flags: Flags, sys_time: u64, data: &[u8]) -> DeviceReply {
        DeviceReply {
            device,
            msg_id: 1,
            status,
            flags,
            unit_id: 0,
            sys_time,
            data: data.to_vec(),
        }
    }

    fn replied(num_devices: usize, replies: &[(usize, u64, &[u8])]) -> Response {
        let mut response = Response::for_devices(num_devices);
        for &(device, sys_time, data) in replies {
            response.record_reply(reply(device, 0, Flags::ASSIGNED, sys_time, data));
        }
        response
    }

    #[test]
    fn a_recorded_reply_marks_only_its_device_as_replied() {
        let mut response = Response::for_devices(3);
        response.record_reply(reply(1, 0x07, Flags::ASSIGNED, 2_000, &[]));
        assert_eq!(response.replied_devices(), DeviceSet::from_iter([1]));
        assert!(!response.has_all_replies());
        assert_eq!(response.error_codes(), [0, 0x07, 0]);
        assert_eq!(response.sys_time_of(0), None);
        assert_eq!(response.sys_time_of(1), Some(SysTime::from_nanos(2_000)));
    }

    #[test]
    fn every_device_replying_completes_the_response() {
        let response = replied(2, &[(1, 0, &[]), (0, 0, &[])]);
        assert!(response.has_all_replies());
    }

    #[test]
    fn the_device_status_follows_the_flags_of_each_reply() {
        let mut response = Response::for_devices(3);
        response.record_reply(reply(
            0,
            0,
            Flags::ASSIGNED | Flags::PTP_LOCKED | Flags::SYNC_READY,
            0,
            &[],
        ));
        response.record_reply(reply(1, 0, Flags::ASSIGNED | Flags::PTP_LOCKED, 0, &[]));
        response.record_reply(reply(2, 0, Flags::ASSIGNED | Flags::FAILSAFE, 0, &[]));
        assert_eq!(
            response.device_status(),
            [
                DeviceStatus {
                    state: DeviceState::Ready,
                    ptp_locked: true,
                    failsafe_active: false,
                },
                DeviceStatus {
                    state: DeviceState::Syncing,
                    ptp_locked: true,
                    failsafe_active: false,
                },
                DeviceStatus {
                    state: DeviceState::Syncing,
                    ptp_locked: false,
                    failsafe_active: true,
                },
            ]
        );
    }

    #[test]
    fn reply_data_stays_empty_until_a_device_sends_some() {
        assert_eq!(
            replied(2, &[(0, 0, &[]), (1, 0, &[])]).reply_data().len(),
            0
        );
        assert_eq!(
            replied(3, &[(1, 0, &[9])]).reply_data(),
            [vec![], vec![9], vec![]]
        );
    }

    #[test]
    fn error_codes_expose_only_the_recorded_devices() {
        assert_eq!(
            Response::from_error_codes(&[0x00, 0xAB]).error_codes(),
            [0x00, 0xAB]
        );
        assert_eq!(Response::from_error_codes(&[]).error_codes(), [0u8; 0]);
    }

    #[test]
    fn from_error_codes_clamps_to_the_device_limit() {
        let response = Response::from_error_codes(&[1u8; MAX_DEVICES + 4]);
        assert_eq!(response.error_codes().len(), MAX_DEVICES);
    }

    #[test]
    fn the_device_time_of_a_reply_is_per_device() {
        let response = replied(3, &[(0, 5_000, &[]), (1, 0, &[])]);
        assert_eq!(response.sys_time_of(0), Some(SysTime::from_nanos(5_000)));
        assert_eq!(response.sys_time_of(1), None);
        assert_eq!(response.sys_time_of(2), None);
        assert_eq!(response.sys_time_of(3), None);
        assert_eq!(Response::from_error_codes(&[0]).sys_time_of(0), None);
    }

    #[test]
    fn reply_data_is_per_device() {
        let response = replied(2, &[(0, 0, &[1, 2]), (1, 0, &[3])]);
        assert_eq!(response.reply_data_of(0), [1, 2]);
        assert_eq!(response.reply_data_of(1), [3]);
        assert_eq!(response.reply_data_of(2), [0u8; 0]);
        assert_eq!(Response::from_error_codes(&[0]).reply_data_of(0), [0u8; 0]);
    }

    #[test]
    fn parse_each_device_returns_one_value_per_device() {
        let response = replied(2, &[(0, 0, &[1, 2]), (1, 0, &[3])]);
        let firsts = response
            .parse_each_device(|value| value.first().copied())
            .unwrap();
        assert_eq!(firsts, [1, 3]);
    }

    #[test]
    fn parse_each_device_reports_a_short_reply_as_unexpected() {
        let response = replied(3, &[(0, 0, &[1]), (1, 0, &[]), (2, 0, &[])]);
        let err = response
            .parse_each_device(|value| value.first().copied())
            .unwrap_err();
        assert!(matches!(err, Error::UnexpectedReply { device: 1 }));
    }

    #[test]
    fn the_replied_mask_covers_every_device_up_to_the_limit() {
        let full = Response::from_error_codes(&[0u8; MAX_DEVICES]);
        assert_eq!(
            full.parse_each_device(|_| Some(())).unwrap().len(),
            MAX_DEVICES
        );
        assert_eq!(
            Response::default()
                .parse_each_device(|_| Some(()))
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn check_reports_the_first_nonzero_device() {
        assert!(Response::from_error_codes(&[0, 0, 0]).check().is_ok());
        let err = Response::from_error_codes(&[0, 0x07, 0x09])
            .check()
            .unwrap_err();
        assert!(matches!(
            err,
            Error::DeviceError {
                device: 1,
                code: 0x07
            }
        ));
    }
}
