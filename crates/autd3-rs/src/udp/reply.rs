use autd3_cpu_wire::udp::{Flags, FrameReply, Header, Kind, PROTOCOL_VERSION};
use autd3_rs_core::REPLY_DATA_BYTES_MAX;
use zerocopy::FromBytes;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReplyData {
    len: u8,
    bytes: [u8; REPLY_DATA_BYTES_MAX],
}

impl ReplyData {
    #[must_use]
    pub(crate) fn new(data: &[u8]) -> Self {
        let len = data.len().min(REPLY_DATA_BYTES_MAX);
        let mut bytes = [0; REPLY_DATA_BYTES_MAX];
        bytes[..len].copy_from_slice(&data[..len]);
        Self {
            len: u8::try_from(len).expect("bounded by REPLY_DATA_BYTES_MAX"),
            bytes,
        }
    }

    #[must_use]
    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Reply {
    pub(crate) device: usize,
    pub(crate) msg_id: u16,
    pub(crate) error_code: u8,
    pub(crate) sys_time: u64,
    pub(crate) data: ReplyData,
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
        Self {
            device,
            msg_id,
            error_code,
            sys_time,
            data: ReplyData::new(data),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DeviceReply {
    pub(crate) reply: Reply,
    pub(crate) reported_unit_id: u8,
    pub(crate) flags: Flags,
}

impl DeviceReply {
    #[must_use]
    pub(crate) fn parse(device: usize, header: Header, body: &[u8]) -> Option<Self> {
        if header.version != PROTOCOL_VERSION
            || (header.kind != Kind::Frame.as_u8() && header.kind != Kind::Heartbeat.as_u8())
        {
            return None;
        }
        let (head, data) = FrameReply::read_from_prefix(body).ok()?;
        Some(Self {
            reply: Reply::new(
                device,
                header.msg_id.get(),
                head.status,
                head.sys_time.get(),
                data,
            ),
            reported_unit_id: head.unit_id,
            flags: head.flags,
        })
    }
}

#[cfg(test)]
mod tests {
    use zerocopy::IntoBytes;
    use zerocopy::little_endian::U64;

    use super::*;

    fn reply_body(data: &[u8]) -> Vec<u8> {
        let head = FrameReply {
            reserved: 0,
            status: 3,
            flags: Flags::ASSIGNED,
            unit_id: 2,
            sys_time: U64::new(1_000),
        };
        [head.as_bytes(), data].concat()
    }

    #[test]
    fn a_frame_reply_is_read_with_its_device_and_flags() {
        let parsed =
            DeviceReply::parse(2, Header::new(Kind::Frame, 9), &reply_body(&[7, 8])).unwrap();
        assert_eq!(parsed.reply, Reply::new(2, 9, 3, 1_000, &[7, 8]));
        assert_eq!(parsed.reported_unit_id, 2);
        assert_eq!(parsed.flags, Flags::ASSIGNED);
    }

    #[test]
    fn a_reply_of_another_kind_version_or_length_is_not_read() {
        let body = reply_body(&[]);
        assert!(DeviceReply::parse(0, Header::new(Kind::Heartbeat, 1), &body).is_some());
        assert!(DeviceReply::parse(0, Header::new(Kind::Discover, 1), &body).is_none());
        let mut other_version = Header::new(Kind::Frame, 1);
        other_version.version = PROTOCOL_VERSION.wrapping_add(1);
        assert!(DeviceReply::parse(0, other_version, &body).is_none());
        assert!(
            DeviceReply::parse(0, Header::new(Kind::Frame, 1), &body[..body.len() - 1]).is_none()
        );
    }

    #[test]
    fn data_is_clamped_to_the_reply_limit() {
        let reply = Reply::new(1, 2, 0, 0, &[7; REPLY_DATA_BYTES_MAX + 5]);
        assert_eq!(reply.data.as_slice().len(), REPLY_DATA_BYTES_MAX);
        assert_eq!(Reply::new(0, 0, 0, 0, &[]).data.as_slice(), [0u8; 0]);
    }
}
