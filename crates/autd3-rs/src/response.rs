use crate::client::MAX_DEVICES;
use crate::error::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    status: [u8; MAX_DEVICES],
    len: usize,
    values: Vec<Vec<u8>>,
}

impl Default for Response {
    fn default() -> Self {
        Self {
            status: [0; MAX_DEVICES],
            len: 0,
            values: Vec::new(),
        }
    }
}

impl Response {
    #[must_use]
    pub fn from_status(status: &[u8]) -> Self {
        let len = status.len().min(MAX_DEVICES);
        let mut buf = [0u8; MAX_DEVICES];
        buf[..len].copy_from_slice(&status[..len]);
        Self {
            status: buf,
            len,
            values: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_values(status: &[u8], values: Vec<Vec<u8>>) -> Self {
        let mut response = Self::from_status(status);
        response.values = values;
        response.values.truncate(response.len);
        response
    }

    #[must_use]
    pub fn status(&self) -> &[u8] {
        &self.status[..self.len]
    }

    #[must_use]
    pub fn values(&self) -> &[Vec<u8>] {
        &self.values
    }

    #[must_use]
    pub fn value(&self, device: usize) -> &[u8] {
        self.values.get(device).map_or(&[], Vec::as_slice)
    }

    pub fn merge(&mut self, other: &Response) {
        self.status[..self.len]
            .iter_mut()
            .zip(other.status().iter().copied())
            .for_each(|(m, d)| {
                if *m == 0 {
                    *m = d;
                }
            });
    }

    pub fn check(&self) -> Result<(), Error> {
        match self.status().iter().enumerate().find(|&(_, &d)| d != 0) {
            None => Ok(()),
            Some((device, &code)) => Err(Error::DeviceError { device, code }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Response;

    #[test]
    fn status_exposes_only_the_recorded_devices() {
        assert_eq!(Response::from_status(&[0x00, 0xAB]).status(), [0x00, 0xAB]);
        assert_eq!(Response::from_status(&[]).status(), [0u8; 0]);
    }

    #[test]
    fn from_status_clamps_to_the_device_limit() {
        let response = Response::from_status(&[1u8; crate::client::MAX_DEVICES + 4]);
        assert_eq!(response.status().len(), crate::client::MAX_DEVICES);
    }

    #[test]
    fn values_are_per_device() {
        let response = Response::with_values(&[0, 0], vec![vec![1, 2], vec![3]]);
        assert_eq!(response.value(0), [1, 2]);
        assert_eq!(response.value(1), [3]);
        assert_eq!(response.value(2), [0u8; 0]);
        assert_eq!(Response::from_status(&[0]).value(0), [0u8; 0]);
    }

    #[test]
    fn check_reports_the_first_nonzero_device() {
        assert!(Response::from_status(&[0, 0, 0]).check().is_ok());
        let err = Response::from_status(&[0, 0x07, 0x09]).check().unwrap_err();
        assert!(matches!(
            err,
            crate::error::Error::DeviceError {
                device: 1,
                code: 0x07
            }
        ));
    }

    #[test]
    fn merge_keeps_the_first_error_of_each_device() {
        let mut merged = Response::from_status(&[0, 0x02, 0]);
        merged.merge(&Response::from_status(&[0x05, 0x06, 0]));
        assert_eq!(merged.status(), [0x05, 0x02, 0]);
    }
}
