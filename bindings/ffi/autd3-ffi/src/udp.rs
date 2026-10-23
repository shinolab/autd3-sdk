use std::ffi::c_char;
use std::net::SocketAddrV6;
use std::num::NonZeroUsize;
use std::time::Duration;

use autd3_ffi_abi::{cstr_to_string, to_ns, write_out};
use autd3_rs::Interface;
use autd3_rs::udp::TransportOption;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Autd3TransportOption {
    pub open_timeout_ns: u64,
    pub lost_timeout_ns: u64,
    pub ack_timeout_ns: u64,
    pub send_buffer_bytes: u64,
    pub sync_on_open: bool,
}

impl From<&TransportOption> for Autd3TransportOption {
    fn from(option: &TransportOption) -> Self {
        Self {
            open_timeout_ns: to_ns(option.open_timeout),
            lost_timeout_ns: to_ns(option.lost_timeout),
            ack_timeout_ns: to_ns(option.ack_timeout),
            send_buffer_bytes: option
                .send_buffer_bytes
                .map_or(0, |bytes| bytes.get() as u64),
            sync_on_open: option.sync_on_open,
        }
    }
}

pub(crate) unsafe fn to_transport_option(
    iface_kind: u8,
    iface: *const c_char,
    option: Autd3TransportOption,
) -> Result<TransportOption, String> {
    let iface = match iface_kind {
        0 => Interface::Auto,
        1 | 2 => {
            let Some(value) = (unsafe { cstr_to_string(iface) }) else {
                return Err("null iface".to_owned());
            };
            if iface_kind == 1 {
                Interface::Name(value)
            } else {
                Interface::Addr(
                    value
                        .parse::<SocketAddrV6>()
                        .map_err(|_| format!("iface is not an IPv6 socket address: {value}"))?,
                )
            }
        }
        kind => return Err(format!("unknown iface kind: {kind}")),
    };
    let send_buffer_bytes = usize::try_from(option.send_buffer_bytes).map_err(|_| {
        format!(
            "send_buffer_bytes is out of range: {}",
            option.send_buffer_bytes
        )
    })?;
    Ok(TransportOption {
        iface,
        open_timeout: Duration::from_nanos(option.open_timeout_ns),
        lost_timeout: Duration::from_nanos(option.lost_timeout_ns),
        ack_timeout: Duration::from_nanos(option.ack_timeout_ns),
        send_buffer_bytes: NonZeroUsize::new(send_buffer_bytes),
        sync_on_open: option.sync_on_open,
    })
}

#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn autd3_transport_option_default(
    out: *mut Autd3TransportOption,
) -> i32 {
    unsafe { write_out(out, (&TransportOption::default()).into()) }
}

#[cfg(test)]
mod tests {
    use std::ffi::CString;

    use autd3_ffi_abi::{AUTD3_ERR_INVALID_ARGUMENT, AUTD3_OK};
    use autd3_rs::Geometry;
    use autd3_rs::geometry::Autd3;

    use super::*;

    fn default_option() -> Autd3TransportOption {
        let mut option = Autd3TransportOption {
            open_timeout_ns: 0,
            lost_timeout_ns: 0,
            ack_timeout_ns: 0,
            send_buffer_bytes: 0,
            sync_on_open: false,
        };
        assert_eq!(
            unsafe { autd3_transport_option_default(&raw mut option) },
            AUTD3_OK
        );
        option
    }

    fn convert(kind: u8, iface: Option<&str>) -> Result<TransportOption, String> {
        let iface = iface.map(|s| CString::new(s).unwrap());
        unsafe {
            to_transport_option(
                kind,
                iface.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
                default_option(),
            )
        }
    }

    #[test]
    fn the_defaults_reach_c_unchanged() {
        assert_eq!(convert(0, None), Ok(TransportOption::default()));
        assert_eq!(
            unsafe { autd3_transport_option_default(std::ptr::null_mut()) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
    }

    #[test]
    fn every_field_reaches_the_rust_option() {
        let option = Autd3TransportOption {
            open_timeout_ns: 1,
            lost_timeout_ns: 2,
            ack_timeout_ns: 3,
            send_buffer_bytes: 0,
            sync_on_open: false,
        };
        assert_eq!(
            unsafe { to_transport_option(0, std::ptr::null(), option) },
            Ok(TransportOption {
                iface: Interface::Auto,
                open_timeout: Duration::from_nanos(1),
                lost_timeout: Duration::from_nanos(2),
                ack_timeout: Duration::from_nanos(3),
                send_buffer_bytes: None,
                sync_on_open: false,
            })
        );
    }

    #[test]
    fn the_interface_kinds_map_to_the_rust_variants() {
        assert_eq!(
            convert(1, Some("eth0")).map(|o| o.iface),
            Ok(Interface::Name("eth0".to_owned()))
        );
        assert_eq!(
            convert(2, Some("[::1]:44336")).map(|o| o.iface),
            Ok(Interface::Addr("[::1]:44336".parse().unwrap()))
        );
        assert_eq!(
            convert(2, Some("127.0.0.1:1")),
            Err("iface is not an IPv6 socket address: 127.0.0.1:1".to_owned())
        );
        assert_eq!(convert(1, None), Err("null iface".to_owned()));
        assert_eq!(convert(2, None), Err("null iface".to_owned()));
        assert_eq!(convert(3, None), Err("unknown iface kind: 3".to_owned()));
        assert_eq!(
            convert(0, Some("eth0")).map(|o| o.iface),
            Ok(Interface::Auto)
        );
    }

    fn open(
        geometry: *const Geometry,
        iface_kind: u8,
        out: bool,
    ) -> (i32, *mut crate::ClientHandle, String) {
        let mut client = std::ptr::NonNull::<crate::ClientHandle>::dangling().as_ptr();
        let mut err = [0 as c_char; 256];
        let code = unsafe {
            crate::autd3_client_open(
                geometry,
                iface_kind,
                std::ptr::null(),
                default_option(),
                if out {
                    &raw mut client
                } else {
                    std::ptr::null_mut()
                },
                err.as_mut_ptr(),
                err.len(),
            )
        };
        let message = unsafe { std::ffi::CStr::from_ptr(err.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        (code, client, message)
    }

    #[test]
    fn a_rejected_client_open_reports_the_reason() {
        let (code, client, message) = open(std::ptr::null(), 0, true);
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        assert!(client.is_null());
        assert_eq!("null argument", message);

        let geometry = Geometry::new(vec![Autd3::default()]);
        let (code, _, message) = open(&raw const geometry, 0, false);
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        assert_eq!("null argument", message);

        let (code, client, message) = open(&raw const geometry, 1, true);
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        assert!(client.is_null());
        assert_eq!("null iface", message);
    }

    #[test]
    fn an_out_of_range_device_count_fails_the_open() {
        let geometry = Geometry::new(Vec::<Autd3>::new());
        let (code, client, message) = open(&raw const geometry, 0, true);
        assert_eq!(crate::AUTD3_ERR_NETWORK, code);
        assert!(client.is_null());
        assert_eq!(
            "network error: the number of devices must be 1..=128, but 0 was given",
            message
        );
    }
}
