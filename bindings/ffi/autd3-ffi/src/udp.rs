use std::ffi::c_char;
use std::net::SocketAddrV6;
use std::num::NonZeroUsize;
use std::time::Duration;

use autd3_ffi_abi::{
    AUTD3_ERR_INVALID_ARGUMENT, AUTD3_OK, alloc_cstring, cstr_to_string, free_cstring, handle_mut,
    handle_ref, into_handle, into_handle_or_err, write_out,
};
use autd3_rs::Interface;
use autd3_rs::ServerLink as CoreServerLink;
use autd3_rs::udp::UdpOption as CoreOption;
use autd3_rs_firmware_emulator::udp::UdpEmulator;

pub struct UdpOptionHandle(pub(crate) CoreOption);

#[unsafe(no_mangle)]
pub extern "C" fn autd3_udp_option_new() -> *mut UdpOptionHandle {
    into_handle(UdpOptionHandle(CoreOption::default()))
}

autd3_ffi_abi::option_handle_field!(
    UdpOptionHandle,
    [heartbeat],
    optional_duration,
    autd3_udp_option_set_heartbeat,
    autd3_udp_option_get_heartbeat
);
autd3_ffi_abi::option_handle_field!(
    UdpOptionHandle,
    [recovery_interval],
    optional_duration,
    autd3_udp_option_set_recovery_interval,
    autd3_udp_option_get_recovery_interval
);
autd3_ffi_abi::option_handle_field!(
    UdpOptionHandle,
    [timer_resolution],
    optional_duration,
    autd3_udp_option_set_timer_resolution,
    autd3_udp_option_get_timer_resolution
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_udp_option_set_send_rate_limit(
    handle: *mut UdpOptionHandle,
    has_value: bool,
    percent: f32,
) -> i32 {
    let Some(option) = (unsafe { autd3_ffi_abi::handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    option.0.send_rate_limit = has_value.then_some(percent);
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_udp_option_get_send_rate_limit(
    handle: *const UdpOptionHandle,
    out_has_value: *mut bool,
    out: *mut f32,
) -> i32 {
    let Some(option) = (unsafe { autd3_ffi_abi::handle_ref(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let code =
        unsafe { autd3_ffi_abi::write_out(out_has_value, option.0.send_rate_limit.is_some()) };
    if code != AUTD3_OK {
        return code;
    }
    unsafe { autd3_ffi_abi::write_out(out, option.0.send_rate_limit.unwrap_or(0.0)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_udp_option_set_send_buffer(
    handle: *mut UdpOptionHandle,
    bytes: u64,
) -> i32 {
    let Some(option) = (unsafe { autd3_ffi_abi::handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let Ok(bytes) = usize::try_from(bytes) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    option.0.send_buffer = NonZeroUsize::new(bytes);
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_udp_option_get_send_buffer(
    handle: *const UdpOptionHandle,
    out: *mut u64,
) -> i32 {
    let Some(option) = (unsafe { autd3_ffi_abi::handle_ref(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe {
        autd3_ffi_abi::write_out(
            out,
            option.0.send_buffer.map_or(0, |bytes| bytes.get() as u64),
        )
    }
}
autd3_ffi_abi::option_handle_field!(
    UdpOptionHandle,
    [reply_timeout],
    duration,
    autd3_udp_option_set_reply_timeout,
    autd3_udp_option_get_reply_timeout
);
autd3_ffi_abi::option_handle_field!(
    UdpOptionHandle,
    [lost_timeout],
    duration,
    autd3_udp_option_set_lost_timeout,
    autd3_udp_option_get_lost_timeout
);
autd3_ffi_abi::option_handle_field!(
    UdpOptionHandle,
    [response_timeout],
    duration,
    autd3_udp_option_set_response_timeout,
    autd3_udp_option_get_response_timeout
);
autd3_ffi_abi::option_handle_field!(
    UdpOptionHandle,
    [enumeration_timeout],
    duration,
    autd3_udp_option_set_enumeration_timeout,
    autd3_udp_option_get_enumeration_timeout
);
autd3_ffi_abi::option_handle_field!(
    UdpOptionHandle,
    [sync_timeout],
    duration,
    autd3_udp_option_set_sync_timeout,
    autd3_udp_option_get_sync_timeout
);
autd3_ffi_abi::option_handle_lifecycle!(UdpOptionHandle, autd3_udp_option_free);

pub struct ServerLinkHandle(pub(crate) CoreServerLink);

#[unsafe(no_mangle)]
pub extern "C" fn autd3_server_link_new() -> *mut ServerLinkHandle {
    into_handle(ServerLinkHandle(CoreServerLink::default()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_server_link_set_port(
    handle: *mut ServerLinkHandle,
    port: u16,
) -> i32 {
    let Some(link) = (unsafe { handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    link.0.port = port;
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_server_link_get_port(
    handle: *const ServerLinkHandle,
    out: *mut u16,
) -> i32 {
    let Some(link) = (unsafe { handle_ref(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe { write_out(out, link.0.port) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_server_link_set_timeout_ns(
    handle: *mut ServerLinkHandle,
    ns: u64,
) -> i32 {
    let Some(link) = (unsafe { handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    link.0.timeout = Duration::from_nanos(ns);
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_server_link_get_timeout_ns(
    handle: *const ServerLinkHandle,
    out: *mut u64,
) -> i32 {
    let Some(link) = (unsafe { handle_ref(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe { write_out(out, autd3_ffi_abi::to_ns(link.0.timeout)) }
}

autd3_ffi_abi::option_handle_lifecycle!(ServerLinkHandle, autd3_server_link_free);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_udp_option_set_iface(
    handle: *mut UdpOptionHandle,
    iface: *const c_char,
) -> i32 {
    let Some(option) = (unsafe { handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    option.0.iface = Interface::from(unsafe { cstr_to_string(iface) });
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_udp_option_set_iface_simulator(
    handle: *mut UdpOptionHandle,
    port: u16,
) -> i32 {
    let Some(option) = (unsafe { handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    option.0.iface = Interface::Simulator(port);
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_udp_option_set_iface_addr(
    handle: *mut UdpOptionHandle,
    addr: *const c_char,
) -> i32 {
    let Some(option) = (unsafe { handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let Some(Ok(addr)) = (unsafe { cstr_to_string(addr) }).map(|addr| addr.parse::<SocketAddrV6>())
    else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    option.0.iface = Interface::Addr(addr);
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_udp_option_get_iface_addr(
    handle: *const UdpOptionHandle,
    out: *mut *mut c_char,
) -> i32 {
    let Some(option) = (unsafe { handle_ref(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let value = match &option.0.iface {
        Interface::Addr(addr) => alloc_cstring(&addr.to_string()),
        _ => std::ptr::null_mut(),
    };
    let code = unsafe { write_out(out, value) };
    if code != AUTD3_OK {
        unsafe { free_cstring(value) };
    }
    code
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_udp_free_string(ptr: *mut c_char) {
    unsafe { free_cstring(ptr) };
}

pub struct UdpEmulatorHandle(UdpEmulator);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_udp_emulator_spawn(
    num_devices: usize,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut UdpEmulatorHandle {
    unsafe {
        into_handle_or_err(
            UdpEmulator::spawn(num_devices).map(UdpEmulatorHandle),
            out_err,
            out_err_len,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_udp_emulator_option(
    handle: *const UdpEmulatorHandle,
) -> *mut UdpOptionHandle {
    let Some(emulator) = (unsafe { handle_ref(handle) }) else {
        return std::ptr::null_mut();
    };
    into_handle(UdpOptionHandle(CoreOption::for_emulator(
        emulator.0.interface(),
    )))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_udp_emulator_num_devices(handle: *const UdpEmulatorHandle) -> usize {
    unsafe { handle_ref(handle) }.map_or(0, |emulator| emulator.0.num_devices())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_udp_emulator_reboot(
    handle: *const UdpEmulatorHandle,
    index: usize,
) -> i32 {
    let Some(emulator) = (unsafe { handle_ref(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    if index >= emulator.0.num_devices() {
        return AUTD3_ERR_INVALID_ARGUMENT;
    }
    emulator.0.reboot(index);
    AUTD3_OK
}

autd3_ffi_abi::option_handle_lifecycle!(UdpEmulatorHandle, autd3_udp_emulator_free);

#[cfg(test)]
mod tests {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::ffi::CString;
    use std::time::Duration;

    use autd3_rs::Geometry;
    use autd3_rs::geometry::Autd3;

    use super::*;

    fn optional(
        get: unsafe extern "C" fn(*const UdpOptionHandle, *mut bool, *mut u64) -> i32,
        handle: *const UdpOptionHandle,
    ) -> Option<Duration> {
        let mut has_value = false;
        let mut ns = u64::MAX;
        assert_eq!(
            unsafe { get(handle, &raw mut has_value, &raw mut ns) },
            AUTD3_OK
        );
        has_value.then(|| Duration::from_nanos(ns))
    }

    fn heartbeat(handle: *const UdpOptionHandle) -> Option<Duration> {
        optional(autd3_udp_option_get_heartbeat, handle)
    }

    fn recovery_interval(handle: *const UdpOptionHandle) -> Option<Duration> {
        optional(autd3_udp_option_get_recovery_interval, handle)
    }

    fn timer_resolution(handle: *const UdpOptionHandle) -> Option<Duration> {
        optional(autd3_udp_option_get_timer_resolution, handle)
    }

    #[test]
    fn the_recovery_is_off_until_an_interval_is_set() {
        let handle = autd3_udp_option_new();
        assert_eq!(recovery_interval(handle), None);
        assert_eq!(
            unsafe { autd3_udp_option_set_recovery_interval(handle, true, 100_000_000) },
            AUTD3_OK
        );
        assert_eq!(recovery_interval(handle), Some(Duration::from_millis(100)));
        assert_eq!(
            unsafe { autd3_udp_option_set_recovery_interval(handle, false, 100_000_000) },
            AUTD3_OK
        );
        assert_eq!(recovery_interval(handle), None);
        assert_eq!(
            unsafe { autd3_udp_option_set_recovery_interval(std::ptr::null_mut(), true, 1) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        unsafe { autd3_udp_option_free(handle) };
    }

    #[test]
    fn the_heartbeat_presence_is_explicit_and_a_zero_heartbeat_is_kept_for_validation() {
        let handle = autd3_udp_option_new();
        assert_eq!(heartbeat(handle), Some(Duration::from_millis(10)));
        assert_eq!(
            unsafe { autd3_udp_option_set_heartbeat(handle, false, 0) },
            AUTD3_OK
        );
        assert_eq!(heartbeat(handle), None);
        assert_eq!(
            unsafe { autd3_udp_option_set_heartbeat(handle, true, 0) },
            AUTD3_OK
        );
        assert_eq!(heartbeat(handle), Some(Duration::ZERO));
        assert!(unsafe { &*handle }.0.validate().is_err());
        assert_eq!(
            unsafe { autd3_udp_option_set_heartbeat(handle, true, 2_000_000) },
            AUTD3_OK
        );
        assert_eq!(heartbeat(handle), Some(Duration::from_millis(2)));
        unsafe { autd3_udp_option_free(handle) };
    }

    #[test]
    fn the_timer_resolution_presence_is_explicit() {
        let handle = autd3_udp_option_new();
        assert_eq!(timer_resolution(handle), Some(Duration::from_millis(1)));
        assert_eq!(
            unsafe { autd3_udp_option_set_timer_resolution(handle, false, 0) },
            AUTD3_OK
        );
        assert_eq!(unsafe { &*handle }.0.timer_resolution, None);
        assert_eq!(timer_resolution(handle), None);
        assert_eq!(
            unsafe { autd3_udp_option_set_timer_resolution(handle, true, 2_000_000) },
            AUTD3_OK
        );
        assert_eq!(
            unsafe { &*handle }.0.timer_resolution,
            Some(Duration::from_millis(2))
        );
        assert_eq!(
            unsafe { autd3_udp_option_set_timer_resolution(std::ptr::null_mut(), true, 1) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        unsafe { autd3_udp_option_free(handle) };
    }

    #[test]
    fn the_send_rate_limit_presence_is_explicit() {
        let handle = autd3_udp_option_new();
        let read = || {
            let mut has_value = true;
            let mut percent = f32::NAN;
            assert_eq!(
                unsafe {
                    autd3_udp_option_get_send_rate_limit(
                        handle,
                        &raw mut has_value,
                        &raw mut percent,
                    )
                },
                AUTD3_OK
            );
            has_value.then_some(percent)
        };
        assert_eq!(read(), None);
        assert_eq!(
            unsafe { autd3_udp_option_set_send_rate_limit(handle, true, 95.0) },
            AUTD3_OK
        );
        assert_eq!(read(), Some(95.0));
        assert_eq!(unsafe { &*handle }.0.send_rate_limit, Some(95.0));
        assert_eq!(
            unsafe { autd3_udp_option_set_send_rate_limit(handle, true, 0.0) },
            AUTD3_OK
        );
        assert_eq!(unsafe { &*handle }.0.send_rate_limit, Some(0.0));
        assert!(unsafe { &*handle }.0.validate().is_err());
        assert_eq!(
            unsafe { autd3_udp_option_set_send_rate_limit(handle, false, 0.0) },
            AUTD3_OK
        );
        assert_eq!(unsafe { &*handle }.0.send_rate_limit, None);
        assert_eq!(
            unsafe { autd3_udp_option_set_send_rate_limit(std::ptr::null_mut(), true, 1.0) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        unsafe { autd3_udp_option_free(handle) };
    }

    fn lost_timeout(handle: *const UdpOptionHandle) -> Duration {
        let mut ns = 0u64;
        assert_eq!(
            unsafe { autd3_udp_option_get_lost_timeout(handle, &raw mut ns) },
            AUTD3_OK
        );
        Duration::from_nanos(ns)
    }

    fn iface_addr(handle: *const UdpOptionHandle) -> Option<String> {
        let mut out: *mut c_char = std::ptr::null_mut();
        assert_eq!(
            unsafe { autd3_udp_option_get_iface_addr(handle, &raw mut out) },
            AUTD3_OK
        );
        if out.is_null() {
            return None;
        }
        let s = unsafe { std::ffi::CStr::from_ptr(out) }
            .to_string_lossy()
            .into_owned();
        unsafe { autd3_udp_free_string(out) };
        Some(s)
    }

    #[test]
    fn the_defaults_reach_c_unchanged() {
        let handle = autd3_udp_option_new();
        let option = CoreOption::default();
        assert_eq!(heartbeat(handle), option.heartbeat);
        assert_eq!(lost_timeout(handle), option.lost_timeout);
        let mut ns = 0u64;
        assert_eq!(
            unsafe { autd3_udp_option_get_sync_timeout(handle, &raw mut ns) },
            AUTD3_OK
        );
        assert_eq!(Duration::from_nanos(ns), option.sync_timeout);
        assert_eq!(iface_addr(handle), None);
        unsafe { autd3_udp_option_free(handle) };
    }

    #[test]
    fn the_address_round_trips_and_rejects_garbage() {
        let handle = autd3_udp_option_new();
        let addr = CString::new("[::1]:44336").unwrap();
        assert_eq!(
            unsafe { autd3_udp_option_set_iface_addr(handle, addr.as_ptr()) },
            AUTD3_OK
        );
        assert_eq!(iface_addr(handle).as_deref(), Some("[::1]:44336"));

        let garbage = CString::new("127.0.0.1:1").unwrap();
        assert_eq!(
            unsafe { autd3_udp_option_set_iface_addr(handle, garbage.as_ptr()) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { autd3_udp_option_set_iface_addr(handle, std::ptr::null()) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        assert_eq!(iface_addr(handle).as_deref(), Some("[::1]:44336"));

        assert_eq!(
            unsafe { autd3_udp_option_set_iface(handle, std::ptr::null()) },
            AUTD3_OK
        );
        assert_eq!(iface_addr(handle), None);
        unsafe { autd3_udp_option_free(handle) };
    }

    #[test]
    fn the_simulator_interface_reaches_the_option() {
        let handle = autd3_udp_option_new();
        assert_eq!(
            unsafe { autd3_udp_option_set_iface_simulator(handle, 50000) },
            AUTD3_OK
        );
        assert_eq!(
            unsafe { handle_ref(handle.cast_const()) }.unwrap().0.iface,
            Interface::Simulator(50000)
        );
        assert_eq!(iface_addr(handle), None);
        assert_eq!(
            unsafe { autd3_udp_option_set_iface_simulator(std::ptr::null_mut(), 50000) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        unsafe { autd3_udp_option_free(handle) };
    }

    #[test]
    fn the_server_link_round_trips() {
        let handle = autd3_server_link_new();
        let mut port = 0u16;
        let mut ns = 0u64;
        assert_eq!(
            unsafe { autd3_server_link_get_port(handle, &raw mut port) },
            AUTD3_OK
        );
        assert_eq!(port, autd3_rs::SERVER_PORT);
        assert_eq!(
            unsafe { autd3_server_link_set_port(handle, 5000) },
            AUTD3_OK
        );
        assert_eq!(
            unsafe { autd3_server_link_set_timeout_ns(handle, 2_000_000_000) },
            AUTD3_OK
        );
        assert_eq!(
            unsafe { autd3_server_link_get_port(handle, &raw mut port) },
            AUTD3_OK
        );
        assert_eq!(
            unsafe { autd3_server_link_get_timeout_ns(handle, &raw mut ns) },
            AUTD3_OK
        );
        assert_eq!((port, ns), (5000, 2_000_000_000));
        assert_eq!(
            unsafe { autd3_server_link_set_port(std::ptr::null_mut(), 1) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        unsafe { autd3_server_link_free(handle) };
    }

    #[test]
    fn a_null_handle_is_an_argument_error_not_a_crash() {
        let mut has_value = false;
        let mut ns = 0u64;
        assert_eq!(
            unsafe {
                autd3_udp_option_get_heartbeat(std::ptr::null(), &raw mut has_value, &raw mut ns)
            },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        assert!(unsafe { autd3_udp_emulator_option(std::ptr::null()) }.is_null());
    }

    #[test]
    fn a_rejected_client_open_consumes_the_option() {
        let option = autd3_udp_option_new();
        let config = crate::autd3_client_config_new();
        let (code, client) = complete(|cb, user_data| unsafe {
            crate::autd3_client_open(std::ptr::null(), option, config, cb, user_data);
        });
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        assert_eq!(0, client);
        unsafe { crate::autd3_client_config_free(config) };

        let (code, client) = complete(|cb, user_data| unsafe {
            crate::autd3_client_open(
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                cb,
                user_data,
            );
        });
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        assert_eq!(0, client);
    }

    #[test]
    fn a_server_open_with_a_null_geometry_consumes_the_link() {
        let config = crate::autd3_client_config_new();
        let live = LIVE_ALLOCATIONS.get();
        let link = autd3_server_link_new();
        assert!(live < LIVE_ALLOCATIONS.get());
        let (code, client) = complete(|cb, user_data| unsafe {
            crate::autd3_client_open_server(std::ptr::null(), link, config, cb, user_data);
        });
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        assert_eq!(0, client);
        assert_eq!(live, LIVE_ALLOCATIONS.get());
        unsafe { crate::autd3_client_config_free(config) };
    }

    #[test]
    fn a_zero_server_timeout_fails_the_open_not_the_setter() {
        let geometry = Geometry::new(vec![Autd3::default()]);
        let config = crate::autd3_client_config_new();
        let link = autd3_server_link_new();
        assert_eq!(
            unsafe { autd3_server_link_set_timeout_ns(link, 0) },
            AUTD3_OK
        );
        let mut ns = u64::MAX;
        assert_eq!(
            unsafe { autd3_server_link_get_timeout_ns(link, &raw mut ns) },
            AUTD3_OK
        );
        assert_eq!(ns, 0);
        let (code, client) = complete(|cb, user_data| unsafe {
            crate::autd3_client_open_server(&raw const geometry, link, config, cb, user_data);
        });
        assert_eq!(autd3_ffi_abi::AUTD3_ERR_NETWORK, code);
        assert_eq!(0, client);
        unsafe { crate::autd3_client_config_free(config) };
    }

    #[test]
    fn an_out_of_range_device_count_fails_the_open_through_the_callback() {
        let geometry = Geometry::new(Vec::<Autd3>::new());
        let option = autd3_udp_option_new();
        let config = crate::autd3_client_config_new();
        let (code, client) = complete(|cb, user_data| unsafe {
            crate::autd3_client_open(&raw const geometry, option, config, cb, user_data);
        });
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        assert_eq!(0, client);
        unsafe { crate::autd3_client_config_free(config) };
    }

    #[test]
    fn a_client_open_without_a_callback_still_consumes_the_option() {
        let geometry = Geometry::new(vec![Autd3::default()]);
        let config = crate::autd3_client_config_new();
        let live = LIVE_ALLOCATIONS.get();
        let option = autd3_udp_option_new();
        assert!(live < LIVE_ALLOCATIONS.get());
        unsafe {
            crate::autd3_client_open(
                &raw const geometry,
                option,
                config,
                None,
                std::ptr::null_mut(),
            );
        }
        assert_eq!(live, LIVE_ALLOCATIONS.get());
        unsafe { crate::autd3_client_config_free(config) };
    }

    #[test]
    fn a_token_await_without_a_callback_still_consumes_the_token() {
        let live = LIVE_ALLOCATIONS.get();
        let token = into_handle(crate::ResponseToken(Vec::new()));
        assert!(live < LIVE_ALLOCATIONS.get());
        unsafe { crate::autd3_response_token_await(token, None, std::ptr::null_mut()) };
        assert_eq!(live, LIVE_ALLOCATIONS.get());
    }

    #[test]
    fn the_emulator_option_is_the_shared_emulator_option() {
        let emulator = unsafe { autd3_udp_emulator_spawn(1, std::ptr::null_mut(), 0) };
        assert!(!emulator.is_null());
        let option = unsafe { autd3_udp_emulator_option(emulator) };
        let interface = unsafe { handle_ref(emulator.cast_const()) }
            .unwrap()
            .0
            .interface();
        assert_eq!(
            CoreOption::for_emulator(interface),
            unsafe { handle_ref(option.cast_const()) }.unwrap().0
        );
        assert_eq!(1, unsafe { autd3_udp_emulator_num_devices(emulator) });
        assert_eq!(0, unsafe {
            autd3_udp_emulator_num_devices(std::ptr::null())
        });
        unsafe { autd3_udp_option_free(option) };
        unsafe { autd3_udp_emulator_free(emulator) };
    }

    #[test]
    fn a_length_mismatch_leaves_the_streamed_command_with_the_caller() {
        let emulator = unsafe { autd3_udp_emulator_spawn(1, std::ptr::null_mut(), 0) };
        let geometry = Geometry::new(vec![Autd3::default()]);
        let config = crate::autd3_client_config_new();
        let option = unsafe { autd3_udp_emulator_option(emulator) };
        let (code, client) = complete(|cb, user_data| unsafe {
            crate::autd3_client_open(&raw const geometry, option, config, cb, user_data);
        });
        assert_eq!(AUTD3_OK, code);
        let client = client as *mut crate::ClientHandle;

        let per_device = [crate::autd3_op_nop(), crate::autd3_op_nop()];
        let command = unsafe { crate::autd3_command_each(per_device.as_ptr(), per_device.len()) };
        let (code, stream) = complete(|cb, user_data| unsafe {
            crate::autd3_client_send_streaming(client, command, cb, user_data);
        });
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        assert_eq!(0, stream);

        let two = Geometry::new(vec![Autd3::default(), Autd3::default()]);
        let frames = crate::autd3_frames_new();
        assert_eq!(AUTD3_OK, unsafe {
            crate::autd3_frames_encode_into(
                frames,
                &raw const two,
                command,
                std::ptr::null_mut(),
                0,
            )
        });
        unsafe { crate::autd3_frames_free(frames) };

        let (code, _) = complete(|cb, user_data| unsafe {
            crate::autd3_client_close(client, cb, user_data);
        });
        assert_eq!(AUTD3_OK, code);
        unsafe { crate::autd3_client_free(client) };
        unsafe { crate::autd3_client_config_free(config) };
        unsafe { autd3_udp_emulator_free(emulator) };
    }

    struct CountingAllocator;

    thread_local! {
        static LIVE_ALLOCATIONS: Cell<isize> = const { Cell::new(0) };
    }

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            LIVE_ALLOCATIONS.set(LIVE_ALLOCATIONS.get() + 1);
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            LIVE_ALLOCATIONS.set(LIVE_ALLOCATIONS.get() - 1);
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;

    type Completion = (i32, usize);

    extern "C" fn report(
        code: i32,
        value: *mut std::ffi::c_void,
        _msg: *const c_char,
        user_data: *mut std::ffi::c_void,
    ) {
        let tx = unsafe { Box::from_raw(user_data.cast::<std::sync::mpsc::Sender<Completion>>()) };
        tx.send((code, value as usize)).unwrap();
    }

    fn complete(
        call: impl FnOnce(autd3_ffi_abi::CompletionCallback, *mut std::ffi::c_void),
    ) -> Completion {
        let (tx, rx) = std::sync::mpsc::channel::<Completion>();
        call(Some(report), Box::into_raw(Box::new(tx)).cast());
        rx.recv().unwrap()
    }
}
