use crate::client::MAX_DEVICES;
use crate::error::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Response {
    data: [u8; MAX_DEVICES],
    len: usize,
}

impl Default for Response {
    fn default() -> Self {
        Self {
            data: [0; MAX_DEVICES],
            len: 0,
        }
    }
}

impl Response {
    #[must_use]
    pub fn from_slice(data: &[u8]) -> Self {
        let len = data.len().min(MAX_DEVICES);
        let mut buf = [0u8; MAX_DEVICES];
        buf[..len].copy_from_slice(&data[..len]);
        Self { data: buf, len }
    }

    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data[..self.len]
    }

    #[must_use]
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data[..self.len]
    }

    pub fn check(&self) -> Result<(), Error> {
        match self.data().iter().enumerate().find(|&(_, &d)| d != 0) {
            None => Ok(()),
            Some((device, &code)) => Err(Error::DeviceError { device, code }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Response;

    #[test]
    fn data_exposes_only_the_recorded_devices() {
        assert_eq!(Response::from_slice(&[0x00, 0xAB]).data(), [0x00, 0xAB]);
        assert!(Response::from_slice(&[]).data().is_empty());
    }

    #[test]
    fn from_slice_clamps_to_the_device_limit() {
        let response = Response::from_slice(&[1u8; crate::client::MAX_DEVICES + 4]);
        assert_eq!(response.data().len(), crate::client::MAX_DEVICES);
    }

    #[test]
    fn check_reports_the_first_nonzero_device() {
        assert!(Response::from_slice(&[0, 0, 0]).check().is_ok());
        let err = Response::from_slice(&[0, 0x07, 0x09]).check().unwrap_err();
        assert!(matches!(
            err,
            crate::error::Error::DeviceError {
                device: 1,
                code: 0x07
            }
        ));
    }
}
