use autd3_rs_core::REPLY_DATA_BYTES_MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reply {
    pub device: usize,
    pub msg_id: u16,
    pub ack: u8,
    pub status: u8,
    pub flags: u8,
    len: u8,
    data: [u8; REPLY_DATA_BYTES_MAX],
}

impl Reply {
    #[must_use]
    pub fn new(device: usize, msg_id: u16, ack: u8, status: u8, flags: u8, data: &[u8]) -> Self {
        let len = data.len().min(REPLY_DATA_BYTES_MAX);
        let mut buf = [0; REPLY_DATA_BYTES_MAX];
        buf[..len].copy_from_slice(&data[..len]);
        Self {
            device,
            msg_id,
            ack,
            status,
            flags,
            len: u8::try_from(len).unwrap_or(u8::MAX),
            data: buf,
        }
    }

    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data[..usize::from(self.len)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_is_clamped_to_the_reply_limit() {
        let reply = Reply::new(1, 2, 3, 0, 0, &[7; REPLY_DATA_BYTES_MAX + 5]);
        assert_eq!(reply.data().len(), REPLY_DATA_BYTES_MAX);
        assert!(Reply::new(0, 0, 0, 0, 0, &[]).data().is_empty());
    }
}
