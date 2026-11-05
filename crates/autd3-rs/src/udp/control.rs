use std::net::SocketAddrV6;

use autd3_cpu_wire::udp::{Header, Kind, PROTOCOL_VERSION, Status};

use super::error::UdpError;

pub(crate) struct ControlResponse {
    pub(crate) src: SocketAddrV6,
    pub(crate) status: u8,
    pub(crate) body: Vec<u8>,
}

impl ControlResponse {
    pub(crate) fn parse(
        src: SocketAddrV6,
        header: Header,
        body: &[u8],
        kind: Kind,
        msg_id: u16,
    ) -> Result<Option<Self>, UdpError> {
        if header.kind != kind.as_u8() || header.msg_id.get() != msg_id {
            return Ok(None);
        }
        if header.version != PROTOCOL_VERSION {
            return Err(UdpError::UnsupportedVersion {
                device_version: header.version,
                host_version: PROTOCOL_VERSION,
            });
        }
        let Some((&status, body)) = body.split_first() else {
            return Ok(None);
        };
        Ok(Some(Self {
            src,
            status,
            body: body.to_vec(),
        }))
    }

    pub(crate) fn check_status(&self, kind: Kind, unit_id: usize) -> Result<(), UdpError> {
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
}

#[cfg(test)]
mod tests {
    use std::net::Ipv6Addr;

    use super::*;

    const SRC: SocketAddrV6 = SocketAddrV6::new(Ipv6Addr::LOCALHOST, 1, 0, 0);

    fn parse(
        header: Header,
        body: &[u8],
        kind: Kind,
        msg_id: u16,
    ) -> Result<Option<ControlResponse>, UdpError> {
        ControlResponse::parse(SRC, header, body, kind, msg_id)
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
}
