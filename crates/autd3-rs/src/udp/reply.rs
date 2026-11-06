use autd3_cpu_wire::udp::{Flags, FrameReply, Header, Kind, PROTOCOL_VERSION};
use zerocopy::FromBytes;

use crate::fpga_state::FpgaState;
use crate::telemetry::Telemetry;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceReply {
    pub device: usize,
    pub msg_id: u16,
    pub status: u8,
    pub flags: Flags,
    pub unit_id: u8,
    pub sys_time: u64,
    pub telemetry: Telemetry,
    pub fpga_state: FpgaState,
    pub data: Vec<u8>,
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
            device,
            msg_id: header.msg_id.get(),
            status: head.status,
            flags: head.flags,
            unit_id: head.unit_id,
            sys_time: head.sys_time.get(),
            telemetry: Telemetry::from_wire(head.telemetry),
            fpga_state: FpgaState(head.fpga_state),
            data: data.to_vec(),
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
            telemetry: autd3_cpu_wire::payload::Telemetry {
                fifo_drop: 5.into(),
                ..Default::default()
            },
            fpga_state: 0x0C,
        };
        [head.as_bytes(), data].concat()
    }

    #[test]
    fn a_frame_reply_is_read_with_its_device_and_flags() {
        let parsed =
            DeviceReply::parse(2, Header::new(Kind::Frame, 9), &reply_body(&[7, 8])).unwrap();
        assert_eq!(
            parsed,
            DeviceReply {
                device: 2,
                msg_id: 9,
                status: 3,
                flags: Flags::ASSIGNED,
                unit_id: 2,
                sys_time: 1_000,
                telemetry: Telemetry {
                    fifo_drop: 5,
                    ..Telemetry::default()
                },
                fpga_state: FpgaState(0x0C),
                data: vec![7, 8],
            }
        );
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
}
