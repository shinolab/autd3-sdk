use autd3_rs_core::REPLY_DATA_BYTES_MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Reply {
    pub(crate) device: usize,
    pub(crate) msg_id: u16,
    pub(crate) error_code: u8,
    pub(crate) sys_time: u64,
    len: u8,
    data: [u8; REPLY_DATA_BYTES_MAX],
}

impl Reply {
    #[must_use]
    pub(crate) fn new(
        device: usize,
        msg_id: u16,
        error_code: u8,
        sys_time: u64,
        data: &[u8],
    ) -> Self {
        let len = data.len().min(REPLY_DATA_BYTES_MAX);
        let mut buf = [0; REPLY_DATA_BYTES_MAX];
        buf[..len].copy_from_slice(&data[..len]);
        Self {
            device,
            msg_id,
            error_code,
            sys_time,
            len: u8::try_from(len).unwrap_or(u8::MAX),
            data: buf,
        }
    }

    #[must_use]
    pub(crate) fn data(&self) -> &[u8] {
        &self.data[..usize::from(self.len)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_is_clamped_to_the_reply_limit() {
        let reply = Reply::new(1, 2, 0, 0, &[7; REPLY_DATA_BYTES_MAX + 5]);
        assert_eq!(reply.data().len(), REPLY_DATA_BYTES_MAX);
        assert_eq!(Reply::new(0, 0, 0, 0, &[]).data(), [0u8; 0]);
    }
}
