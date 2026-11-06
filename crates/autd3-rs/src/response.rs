use autd3_cpu_wire::udp::Flags;
use autd3_rs_core::{DeviceState, MAX_DEVICES};

use crate::device_status::DeviceStatus;
use crate::error::Error;
use crate::udp::DeviceReply;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Response {
    num_devices: usize,
    replied_devices: u128,
    replies: Vec<DeviceReply>,
}

const _: () = assert!(MAX_DEVICES <= u128::BITS as usize);

impl Response {
    #[must_use]
    pub(crate) fn for_devices(num_devices: usize) -> Self {
        let num_devices = num_devices.min(MAX_DEVICES);
        Self {
            num_devices,
            replied_devices: 0,
            replies: Vec::with_capacity(num_devices),
        }
    }

    pub(crate) fn record_reply(&mut self, reply: DeviceReply) {
        if reply.device >= self.num_devices || self.replied_devices & (1 << reply.device) != 0 {
            return;
        }
        self.replied_devices |= 1 << reply.device;
        self.replies.push(reply);
        if self.has_all_replies() {
            self.replies
                .sort_unstable_by_key(|recorded| recorded.device);
        }
    }

    #[must_use]
    pub(crate) fn has_all_replies(&self) -> bool {
        self.replies.len() == self.num_devices
    }

    pub(crate) fn parse_each_device<T>(
        &self,
        parse: impl Fn(&[u8]) -> Option<T>,
    ) -> Result<Vec<T>, Error> {
        self.replies
            .iter()
            .map(|reply| {
                parse(&reply.data).ok_or(Error::UnexpectedReply {
                    device: reply.device,
                })
            })
            .collect()
    }

    #[must_use]
    pub fn replies(&self) -> &[DeviceReply] {
        &self.replies
    }

    #[must_use]
    pub fn device_status(&self) -> Vec<DeviceStatus> {
        self.replies
            .iter()
            .map(|reply| DeviceStatus {
                state: if reply.flags.contains(Flags::PTP_LOCKED | Flags::SYNC_READY) {
                    DeviceState::Ready
                } else {
                    DeviceState::Syncing
                },
                ptp_locked: reply.flags.contains(Flags::PTP_LOCKED),
                failsafe_active: reply.flags.contains(Flags::FAILSAFE),
            })
            .collect()
    }

    pub fn check(&self) -> Result<(), Error> {
        match self.replies.iter().find(|reply| reply.status != 0) {
            None => Ok(()),
            Some(reply) => Err(Error::DeviceError {
                device: reply.device,
                code: reply.status,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpga_state::FpgaState;
    use crate::telemetry::Telemetry;

    fn reply(device: usize, status: u8, flags: Flags, data: &[u8]) -> DeviceReply {
        DeviceReply {
            device,
            msg_id: 1,
            status,
            flags,
            unit_id: 0,
            sys_time: 0,
            telemetry: Telemetry::default(),
            fpga_state: FpgaState::default(),
            data: data.to_vec(),
        }
    }

    fn replied(num_devices: usize, replies: &[(usize, u8, &[u8])]) -> Response {
        let mut response = Response::for_devices(num_devices);
        for &(device, status, data) in replies {
            response.record_reply(reply(device, status, Flags::ASSIGNED, data));
        }
        response
    }

    #[test]
    fn replies_are_ordered_by_device_whatever_order_they_arrive_in() {
        let response = replied(3, &[(2, 0, &[]), (0, 0, &[]), (1, 0, &[])]);
        assert!(response.has_all_replies());
        assert_eq!(
            response
                .replies()
                .iter()
                .map(|reply| reply.device)
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
    }

    #[test]
    fn a_missing_device_leaves_the_response_incomplete() {
        let response = replied(3, &[(0, 0, &[]), (2, 0, &[])]);
        assert!(!response.has_all_replies());
    }

    #[test]
    fn only_the_first_reply_of_a_device_is_kept() {
        let response = replied(1, &[(0, 0x01, &[]), (0, 0x02, &[])]);
        assert_eq!(response.replies(), [reply(0, 0x01, Flags::ASSIGNED, &[])]);
    }

    #[test]
    fn a_reply_from_a_device_beyond_the_count_is_ignored() {
        let response = replied(1, &[(1, 0, &[]), (0, 0, &[])]);
        assert_eq!(response.replies(), [reply(0, 0, Flags::ASSIGNED, &[])]);
    }

    #[test]
    fn the_device_status_follows_the_flags_of_each_reply() {
        let mut response = Response::for_devices(3);
        response.record_reply(reply(
            0,
            0,
            Flags::ASSIGNED | Flags::PTP_LOCKED | Flags::SYNC_READY,
            &[],
        ));
        response.record_reply(reply(1, 0, Flags::ASSIGNED | Flags::PTP_LOCKED, &[]));
        response.record_reply(reply(2, 0, Flags::ASSIGNED | Flags::FAILSAFE, &[]));
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
    fn parse_each_device_returns_one_value_per_device() {
        let response = replied(2, &[(1, 0, &[3]), (0, 0, &[1, 2])]);
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
    fn check_reports_the_first_nonzero_device() {
        assert!(replied(2, &[(0, 0, &[]), (1, 0, &[])]).check().is_ok());
        let err = replied(3, &[(2, 0x09, &[]), (1, 0x07, &[]), (0, 0, &[])])
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
