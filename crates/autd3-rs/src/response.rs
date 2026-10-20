use crate::client::MAX_DEVICES;
use crate::error::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    error_codes: [u8; MAX_DEVICES],
    num_devices: usize,
    reply_data: Vec<Vec<u8>>,
    replied_devices: u128,
}

const _: () = assert!(MAX_DEVICES == u128::BITS as usize);

fn first_n_devices_mask(num_devices: usize) -> u128 {
    u128::MAX
        .checked_shr(u32::try_from(MAX_DEVICES - num_devices).unwrap_or(u32::MAX))
        .unwrap_or(0)
}

impl Default for Response {
    fn default() -> Self {
        Self {
            error_codes: [0; MAX_DEVICES],
            num_devices: 0,
            reply_data: Vec::new(),
            replied_devices: 0,
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
            num_devices,
            reply_data: Vec::new(),
            replied_devices: first_n_devices_mask(num_devices),
        }
    }

    #[must_use]
    pub(crate) fn with_replied_devices(mut self, replied_devices: u128) -> Self {
        self.replied_devices = replied_devices & first_n_devices_mask(self.num_devices);
        self
    }

    pub(crate) fn parse_each_device<T>(
        &self,
        parse: impl Fn(&[u8]) -> Option<T>,
    ) -> Result<Vec<T>, Error> {
        let unreplied_devices = !self.replied_devices & first_n_devices_mask(self.num_devices);
        if unreplied_devices != 0 {
            return Err(Error::DeviceLost {
                device: unreplied_devices.trailing_zeros() as usize,
            });
        }
        (0..self.num_devices)
            .map(|device| {
                parse(self.reply_data_of(device)).ok_or(Error::UnexpectedReply { device })
            })
            .collect()
    }

    #[must_use]
    pub(crate) fn with_reply_data(error_codes: &[u8], reply_data: Vec<Vec<u8>>) -> Self {
        let mut response = Self::from_error_codes(error_codes);
        response.reply_data = reply_data;
        response.reply_data.truncate(response.num_devices);
        response
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

    pub fn keep_first_errors_from(&mut self, other: &Response) {
        self.error_codes[..self.num_devices]
            .iter_mut()
            .zip(other.error_codes().iter().copied())
            .for_each(|(kept, other)| {
                if *kept == 0 {
                    *kept = other;
                }
            });
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
    use super::Response;

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
        let response = Response::from_error_codes(&[1u8; crate::client::MAX_DEVICES + 4]);
        assert_eq!(response.error_codes().len(), crate::client::MAX_DEVICES);
    }

    #[test]
    fn reply_data_is_per_device() {
        let response = Response::with_reply_data(&[0, 0], vec![vec![1, 2], vec![3]]);
        assert_eq!(response.reply_data_of(0), [1, 2]);
        assert_eq!(response.reply_data_of(1), [3]);
        assert_eq!(response.reply_data_of(2), [0u8; 0]);
        assert_eq!(Response::from_error_codes(&[0]).reply_data_of(0), [0u8; 0]);
    }

    #[test]
    fn parse_each_device_returns_one_value_per_device() {
        let response = Response::with_reply_data(&[0, 0], vec![vec![1, 2], vec![3]]);
        let firsts = response
            .parse_each_device(|value| value.first().copied())
            .unwrap();
        assert_eq!(firsts, [1, 3]);
    }

    #[test]
    fn parse_each_device_reports_a_short_reply_as_unexpected() {
        let response = Response::with_reply_data(&[0, 0, 0], vec![vec![1], vec![], vec![]]);
        let err = response
            .parse_each_device(|value| value.first().copied())
            .unwrap_err();
        assert!(matches!(
            err,
            crate::error::Error::UnexpectedReply { device: 1 }
        ));
    }

    #[test]
    fn parse_each_device_reports_the_lowest_device_that_did_not_reply_as_lost() {
        let response =
            Response::with_reply_data(&[0, 0, 0, 0], vec![vec![], vec![], vec![], vec![4]])
                .with_replied_devices(0b1001);
        let err = response
            .parse_each_device(|value| value.first().copied())
            .unwrap_err();
        assert!(matches!(err, crate::error::Error::DeviceLost { device: 1 }));
    }

    #[test]
    fn the_replied_mask_covers_every_device_up_to_the_limit() {
        let full = Response::from_error_codes(&[0u8; crate::client::MAX_DEVICES]);
        assert_eq!(
            full.parse_each_device(|_| Some(())).unwrap().len(),
            crate::client::MAX_DEVICES
        );
        assert_eq!(
            Response::default()
                .parse_each_device(|_| Some(()))
                .unwrap()
                .len(),
            0
        );
        assert!(matches!(
            Response::from_error_codes(&[0, 0, 0])
                .with_replied_devices(0b1101)
                .parse_each_device(|_| Some(())),
            Err(crate::error::Error::DeviceLost { device: 1 })
        ));
        let unsent = Response::from_error_codes(&[0, 0]).with_replied_devices(0);
        assert!(matches!(
            unsent.parse_each_device(|_| Some(())),
            Err(crate::error::Error::DeviceLost { device: 0 })
        ));
    }

    #[test]
    fn check_reports_the_first_nonzero_device() {
        assert!(Response::from_error_codes(&[0, 0, 0]).check().is_ok());
        let err = Response::from_error_codes(&[0, 0x07, 0x09])
            .check()
            .unwrap_err();
        assert!(matches!(
            err,
            crate::error::Error::DeviceError {
                device: 1,
                code: 0x07
            }
        ));
    }

    #[test]
    fn keep_first_errors_from_keeps_the_first_error_of_each_device() {
        let mut merged = Response::from_error_codes(&[0, 0x02, 0]);
        merged.keep_first_errors_from(&Response::from_error_codes(&[0x05, 0x06, 0]));
        assert_eq!(merged.error_codes(), [0x05, 0x02, 0]);
    }
}
