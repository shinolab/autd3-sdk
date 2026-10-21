use std::net::SocketAddrV6;

use autd3_cpu_wire::udp::{
    AssignIdBody, Kind, PROTOCOL_VERSION, Role, SetTimeBody, Status, UnblockReply, UnitInfo,
};
use autd3_rs_core::value::SysTime;
use zerocopy::FromBytes;
use zerocopy::little_endian::U64;

use super::channel::ReceivedPacket;
use super::error::UdpError;

#[derive(Clone, Debug)]
pub(crate) struct ControlResponse {
    pub(crate) src: SocketAddrV6,
    pub(crate) status: u8,
    pub(crate) body: Vec<u8>,
}

impl ControlResponse {
    pub(crate) fn parse(
        packet: &ReceivedPacket<'_>,
        kind: Kind,
        msg_id: u16,
    ) -> Result<Option<Self>, UdpError> {
        if packet.header.kind != kind.as_u8() || packet.header.msg_id.get() != msg_id {
            return Ok(None);
        }
        if packet.header.version != PROTOCOL_VERSION {
            return Err(UdpError::UnsupportedVersion {
                device_version: packet.header.version,
                host_version: PROTOCOL_VERSION,
            });
        }
        let Some((&status, body)) = packet.body.split_first() else {
            return Ok(None);
        };
        Ok(Some(Self {
            src: packet.src,
            status,
            body: body.to_vec(),
        }))
    }

    pub(crate) fn check_status(&self, kind: Kind, unit_id: u8) -> Result<(), UdpError> {
        if self.status == Status::Ok.as_u8() {
            Ok(())
        } else {
            Err(UdpError::Rejected {
                kind,
                unit_id,
                status: self.status,
            })
        }
    }

    pub(crate) fn unit_info(&self) -> Option<UnitInfo> {
        UnitInfo::read_from_prefix(&self.body)
            .ok()
            .map(|(info, _)| info)
    }

    pub(crate) fn opened_downstream_link(&self) -> bool {
        UnblockReply::read_from_prefix(&self.body)
            .is_ok_and(|(reply, _)| reply.downstream_link != 0)
    }
}

pub(crate) fn assign_id_body(unit_id: u8, role: Role) -> AssignIdBody {
    AssignIdBody {
        unit_id,
        role: role.as_u8(),
    }
}

pub(crate) fn set_time_body(sys_time: SysTime) -> SetTimeBody {
    SetTimeBody {
        sys_time: U64::new(sys_time.sys_time()),
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv6Addr;

    use autd3_cpu_wire::udp::{Flags, Header};
    use zerocopy::IntoBytes;
    use zerocopy::little_endian::I32;

    use super::*;

    const SRC: SocketAddrV6 = SocketAddrV6::new(Ipv6Addr::LOCALHOST, 1, 0, 0);

    fn parse(
        header: Header,
        body: &[u8],
        kind: Kind,
        msg_id: u16,
    ) -> Result<Option<ControlResponse>, UdpError> {
        ControlResponse::parse(
            &ReceivedPacket {
                src: SRC,
                header,
                body,
            },
            kind,
            msg_id,
        )
    }

    #[test]
    fn a_response_to_the_request_is_split_into_its_status_and_body() {
        let response = parse(
            Header::new(Kind::AssignId, 3),
            &[0x01, 7, 8],
            Kind::AssignId,
            3,
        )
        .unwrap()
        .unwrap();
        assert_eq!(response.src, SRC);
        assert_eq!(response.status, 0x01);
        assert_eq!(response.body, [7, 8]);
    }

    #[test]
    fn a_response_to_another_request_or_without_a_status_is_skipped() {
        assert!(
            parse(Header::new(Kind::Discover, 3), &[0], Kind::AssignId, 3)
                .unwrap()
                .is_none()
        );
        assert!(
            parse(Header::new(Kind::AssignId, 4), &[0], Kind::AssignId, 3)
                .unwrap()
                .is_none()
        );
        assert!(
            parse(Header::new(Kind::AssignId, 3), &[], Kind::AssignId, 3)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_response_to_another_request_is_skipped_whatever_its_version() {
        let mut header = Header::new(Kind::Discover, 1);
        header.version = PROTOCOL_VERSION.wrapping_add(1);
        assert!(
            parse(header, &[0], Kind::ReadUnitInfo, 1)
                .unwrap()
                .is_none()
        );
        assert!(parse(header, &[0], Kind::Discover, 2).unwrap().is_none());
    }

    #[test]
    fn a_response_in_another_protocol_version_is_an_error() {
        let mut header = Header::new(Kind::ReadUnitInfo, 1);
        header.version = PROTOCOL_VERSION.wrapping_add(1);
        assert!(matches!(
            parse(header, &[0], Kind::ReadUnitInfo, 1),
            Err(UdpError::UnsupportedVersion { device_version, host_version })
                if device_version == header.version && host_version == PROTOCOL_VERSION
        ));
    }

    #[test]
    fn a_rejected_request_names_the_unit_and_status() {
        let response = ControlResponse {
            src: SRC,
            status: 0x02,
            body: Vec::new(),
        };
        assert!(matches!(
            response.check_status(Kind::SetTime, 4),
            Err(UdpError::Rejected {
                kind: Kind::SetTime,
                unit_id: 4,
                status: 0x02
            })
        ));
        let accepted = ControlResponse {
            status: Status::Ok.as_u8(),
            ..response
        };
        assert!(accepted.check_status(Kind::SetTime, 4).is_ok());
    }

    #[test]
    fn a_body_shorter_than_its_reply_reads_as_nothing() {
        let response = |body: Vec<u8>| ControlResponse {
            src: SRC,
            status: 0,
            body,
        };
        assert!(response(vec![0]).unit_info().is_none());
        assert!(!response(Vec::new()).opened_downstream_link());
    }

    #[test]
    fn the_unblock_reply_tells_whether_the_downstream_link_opened() {
        let response = |downstream_link: u8| ControlResponse {
            src: SRC,
            status: 0,
            body: vec![downstream_link],
        };
        assert!(!response(0).opened_downstream_link());
        assert!(response(1).opened_downstream_link());
    }

    #[test]
    fn the_unit_info_is_read_from_the_body() {
        let info = UnitInfo {
            unit_id: 3,
            flags: Flags::ASSIGNED,
            upstream_port: 0,
            reserved: 0,
            fw_version: [0, 10, 0],
            reserved2: 0,
            sys_time: U64::new(5),
            ptp_offset_ns: I32::new(-1),
        };
        let response = ControlResponse {
            src: SRC,
            status: 0,
            body: info.as_bytes().to_vec(),
        };
        assert_eq!(response.unit_info(), Some(info));
    }

    #[test]
    fn the_request_bodies_carry_their_fields() {
        let body = assign_id_body(5, Role::Slave);
        assert_eq!((body.unit_id, body.role), (5, Role::Slave.as_u8()));
        assert_eq!(
            set_time_body(SysTime::from_nanos(1_234)).sys_time.get(),
            1_234
        );
    }
}
