use crate::error::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub data: Vec<u8>,
}

impl Response {
    pub fn check(&self) -> Result<(), Error> {
        match self.data.iter().enumerate().find(|&(_, &d)| d != 0) {
            None => Ok(()),
            Some((device, &code)) => Err(Error::DeviceError { device, code }),
        }
    }
}
