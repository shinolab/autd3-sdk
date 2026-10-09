use std::ffi::c_char;
use std::net::SocketAddrV6;
use std::num::NonZeroUsize;
use std::time::Duration;

use autd3_ffi_abi::{
    AUTD3_ERR_INVALID_ARGUMENT, AUTD3_OK, alloc_cstring, cstr_to_string, free_cstring, handle_mut,
    handle_ref, into_handle, into_handle_or_err, write_out,
};
use autd3_rs::Interface;
use autd3_rs::udp::TransportOption as CoreOption;
use autd3_rs_firmware_emulator::udp::UdpEmulator;

pub struct TransportOptionHandle(pub(crate) CoreOption);

#[unsafe(no_mangle)]
pub extern "C" fn autd3_transport_option_new() -> *mut TransportOptionHandle {
    into_handle(TransportOptionHandle(CoreOption::default()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_transport_option_set_heartbeat(
    handle: *mut TransportOptionHandle,
    ns: u64,
) -> i32 {
    let Some(option) = (unsafe { autd3_ffi_abi::handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    option.0.heartbeat = (ns != 0).then(|| Duration::from_nanos(ns));
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_transport_option_get_heartbeat(
    handle: *const TransportOptionHandle,
    out: *mut u64,
) -> i32 {
    let Some(option) = (unsafe { autd3_ffi_abi::handle_ref(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe { autd3_ffi_abi::write_out(out, option.0.heartbeat.map_or(0, autd3_ffi_abi::to_ns)) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_transport_option_set_timer_resolution(
    handle: *mut TransportOptionHandle,
    ns: u64,
) -> i32 {
    let Some(option) = (unsafe { autd3_ffi_abi::handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    option.0.timer_resolution = (ns != 0).then(|| Duration::from_nanos(ns));
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_transport_option_get_timer_resolution(
    handle: *const TransportOptionHandle,
    out: *mut u64,
) -> i32 {
    let Some(option) = (unsafe { autd3_ffi_abi::handle_ref(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe {
        autd3_ffi_abi::write_out(
            out,
            option.0.timer_resolution.map_or(0, autd3_ffi_abi::to_ns),
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_transport_option_set_send_rate_limit(
    handle: *mut TransportOptionHandle,
    percent: f32,
) -> i32 {
    let Some(option) = (unsafe { autd3_ffi_abi::handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    option.0.send_rate_limit = (percent != 0.0).then_some(percent);
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_transport_option_get_send_rate_limit(
    handle: *const TransportOptionHandle,
    out: *mut f32,
) -> i32 {
    let Some(option) = (unsafe { autd3_ffi_abi::handle_ref(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe { autd3_ffi_abi::write_out(out, option.0.send_rate_limit.unwrap_or(0.0)) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_transport_option_set_send_buffer(
    handle: *mut TransportOptionHandle,
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
pub unsafe extern "C" fn autd3_transport_option_get_send_buffer(
    handle: *const TransportOptionHandle,
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
    TransportOptionHandle,
    [reply_timeout],
    duration,
    autd3_transport_option_set_reply_timeout,
    autd3_transport_option_get_reply_timeout
);
autd3_ffi_abi::option_handle_field!(
    TransportOptionHandle,
    [lost_timeout],
    duration,
    autd3_transport_option_set_lost_timeout,
    autd3_transport_option_get_lost_timeout
);
autd3_ffi_abi::option_handle_field!(
    TransportOptionHandle,
    [response_timeout],
    duration,
    autd3_transport_option_set_response_timeout,
    autd3_transport_option_get_response_timeout
);
autd3_ffi_abi::option_handle_field!(
    TransportOptionHandle,
    [enumeration_timeout],
    duration,
    autd3_transport_option_set_enumeration_timeout,
    autd3_transport_option_get_enumeration_timeout
);
autd3_ffi_abi::option_handle_field!(
    TransportOptionHandle,
    [sync_timeout],
    duration,
    autd3_transport_option_set_sync_timeout,
    autd3_transport_option_get_sync_timeout
);
autd3_ffi_abi::option_handle_lifecycle!(TransportOptionHandle, autd3_transport_option_free);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_transport_option_set_iface(
    handle: *mut TransportOptionHandle,
    iface: *const c_char,
) -> i32 {
    let Some(option) = (unsafe { handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    option.0.iface = Interface::from(unsafe { cstr_to_string(iface) });
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_transport_option_set_iface_simulator(
    handle: *mut TransportOptionHandle,
) -> i32 {
    let Some(option) = (unsafe { handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    option.0.iface = Interface::Simulator;
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_transport_option_set_iface_addr(
    handle: *mut TransportOptionHandle,
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
pub unsafe extern "C" fn autd3_transport_option_get_iface_addr(
    handle: *const TransportOptionHandle,
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

fn emulator_option(emulator: &UdpEmulator) -> CoreOption {
    CoreOption {
        iface: emulator.interface(),
        reply_timeout: Duration::from_millis(50),
        response_timeout: Duration::from_millis(50),
        enumeration_timeout: Duration::from_secs(1),
        sync_timeout: Duration::from_secs(5),
        ..CoreOption::default()
    }
}

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
) -> *mut TransportOptionHandle {
    let Some(emulator) = (unsafe { handle_ref(handle) }) else {
        return std::ptr::null_mut();
    };
    into_handle(TransportOptionHandle(emulator_option(&emulator.0)))
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
    use std::ffi::CString;
    use std::time::Duration;

    use autd3_rs::geometry::Autd3;
    use autd3_rs::{DeviceState, Geometry};

    use super::*;

    fn heartbeat(handle: *const TransportOptionHandle) -> Option<Duration> {
        let mut ns = 0u64;
        assert_eq!(
            unsafe { autd3_transport_option_get_heartbeat(handle, &raw mut ns) },
            AUTD3_OK
        );
        (ns != 0).then(|| Duration::from_nanos(ns))
    }

    #[test]
    fn a_zero_heartbeat_disables_the_heartbeat() {
        let handle = autd3_transport_option_new();
        assert_eq!(heartbeat(handle), Some(Duration::from_millis(10)));
        assert_eq!(
            unsafe { autd3_transport_option_set_heartbeat(handle, 0) },
            AUTD3_OK
        );
        assert_eq!(heartbeat(handle), None);
        assert_eq!(
            unsafe { autd3_transport_option_set_heartbeat(handle, 2_000_000) },
            AUTD3_OK
        );
        assert_eq!(heartbeat(handle), Some(Duration::from_millis(2)));
        unsafe { autd3_transport_option_free(handle) };
    }

    #[test]
    fn a_zero_timer_resolution_leaves_the_timer_resolution_alone() {
        let handle = autd3_transport_option_new();
        let read = || {
            let mut ns = u64::MAX;
            assert_eq!(
                unsafe { autd3_transport_option_get_timer_resolution(handle, &raw mut ns) },
                AUTD3_OK
            );
            ns
        };
        assert_eq!(read(), 1_000_000);
        assert_eq!(
            unsafe { autd3_transport_option_set_timer_resolution(handle, 0) },
            AUTD3_OK
        );
        assert_eq!(read(), 0);
        assert_eq!(unsafe { &*handle }.0.timer_resolution, None);
        assert_eq!(
            unsafe { autd3_transport_option_set_timer_resolution(handle, 2_000_000) },
            AUTD3_OK
        );
        assert_eq!(
            unsafe { &*handle }.0.timer_resolution,
            Some(Duration::from_millis(2))
        );
        assert_eq!(
            unsafe { autd3_transport_option_set_timer_resolution(std::ptr::null_mut(), 1) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        unsafe { autd3_transport_option_free(handle) };
    }

    #[test]
    fn a_zero_send_rate_limit_means_no_limit() {
        let handle = autd3_transport_option_new();
        let read = || {
            let mut percent = f32::NAN;
            assert_eq!(
                unsafe { autd3_transport_option_get_send_rate_limit(handle, &raw mut percent) },
                AUTD3_OK
            );
            percent
        };
        assert_eq!(read().to_bits(), 0.0f32.to_bits());
        assert_eq!(
            unsafe { autd3_transport_option_set_send_rate_limit(handle, 95.0) },
            AUTD3_OK
        );
        assert_eq!(read().to_bits(), 95.0f32.to_bits());
        assert_eq!(unsafe { &*handle }.0.send_rate_limit, Some(95.0));
        assert_eq!(
            unsafe { autd3_transport_option_set_send_rate_limit(handle, 0.0) },
            AUTD3_OK
        );
        assert_eq!(unsafe { &*handle }.0.send_rate_limit, None);
        assert_eq!(
            unsafe { autd3_transport_option_set_send_rate_limit(std::ptr::null_mut(), 1.0) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        unsafe { autd3_transport_option_free(handle) };
    }

    fn lost_timeout(handle: *const TransportOptionHandle) -> Duration {
        let mut ns = 0u64;
        assert_eq!(
            unsafe { autd3_transport_option_get_lost_timeout(handle, &raw mut ns) },
            AUTD3_OK
        );
        Duration::from_nanos(ns)
    }

    fn iface_addr(handle: *const TransportOptionHandle) -> Option<String> {
        let mut out: *mut c_char = std::ptr::null_mut();
        assert_eq!(
            unsafe { autd3_transport_option_get_iface_addr(handle, &raw mut out) },
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
        let handle = autd3_transport_option_new();
        let option = CoreOption::default();
        assert_eq!(heartbeat(handle), option.heartbeat);
        assert_eq!(lost_timeout(handle), option.lost_timeout);
        let mut ns = 0u64;
        assert_eq!(
            unsafe { autd3_transport_option_get_sync_timeout(handle, &raw mut ns) },
            AUTD3_OK
        );
        assert_eq!(Duration::from_nanos(ns), option.sync_timeout);
        assert_eq!(iface_addr(handle), None);
        unsafe { autd3_transport_option_free(handle) };
    }

    #[test]
    fn the_address_round_trips_and_rejects_garbage() {
        let handle = autd3_transport_option_new();
        let addr = CString::new("[::1]:44336").unwrap();
        assert_eq!(
            unsafe { autd3_transport_option_set_iface_addr(handle, addr.as_ptr()) },
            AUTD3_OK
        );
        assert_eq!(iface_addr(handle).as_deref(), Some("[::1]:44336"));

        let garbage = CString::new("127.0.0.1:1").unwrap();
        assert_eq!(
            unsafe { autd3_transport_option_set_iface_addr(handle, garbage.as_ptr()) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { autd3_transport_option_set_iface_addr(handle, std::ptr::null()) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        assert_eq!(iface_addr(handle).as_deref(), Some("[::1]:44336"));

        assert_eq!(
            unsafe { autd3_transport_option_set_iface(handle, std::ptr::null()) },
            AUTD3_OK
        );
        assert_eq!(iface_addr(handle), None);
        unsafe { autd3_transport_option_free(handle) };
    }

    #[test]
    fn the_simulator_interface_reaches_the_option() {
        let handle = autd3_transport_option_new();
        assert_eq!(
            unsafe { autd3_transport_option_set_iface_simulator(handle) },
            AUTD3_OK
        );
        assert_eq!(
            unsafe { handle_ref(handle.cast_const()) }.unwrap().0.iface,
            Interface::Simulator
        );
        assert_eq!(iface_addr(handle), None);
        assert_eq!(
            unsafe { autd3_transport_option_set_iface_simulator(std::ptr::null_mut()) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        unsafe { autd3_transport_option_free(handle) };
    }

    #[test]
    fn a_null_handle_is_an_argument_error_not_a_crash() {
        let mut ns = 0u64;
        assert_eq!(
            unsafe { autd3_transport_option_get_heartbeat(std::ptr::null(), &raw mut ns) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        assert!(unsafe { autd3_udp_emulator_option(std::ptr::null()) }.is_null());
    }

    fn spawn_emulator(n: usize) -> *mut UdpEmulatorHandle {
        let mut err = [0 as c_char; 256];
        let emulator = unsafe { autd3_udp_emulator_spawn(n, err.as_mut_ptr(), err.len()) };
        assert!(!emulator.is_null());
        emulator
    }

    fn open_client(emulator: *const UdpEmulatorHandle, n: usize) -> *const crate::ClientHandle {
        let option = unsafe { autd3_udp_emulator_option(emulator) };
        let geometry = Geometry::new(vec![Autd3::default(); n]);
        let config = crate::autd3_client_config_new();
        let (code, client) = complete(|cb, user_data| unsafe {
            crate::autd3_client_open(&raw const geometry, option, config, cb, user_data);
        });
        unsafe { crate::autd3_client_config_free(config) };
        assert_eq!(AUTD3_OK, code);
        assert_ne!(0, client);
        client as *const crate::ClientHandle
    }

    fn close_client(client: *const crate::ClientHandle) {
        let closed = complete(|cb, user_data| unsafe {
            crate::autd3_client_close(client, cb, user_data);
        });
        assert_eq!(AUTD3_OK, closed.0);
        unsafe { crate::autd3_client_free(client.cast_mut()) };
    }

    #[test]
    fn the_emulator_opens_a_client() {
        let emulator = spawn_emulator(2);
        let client = open_client(emulator, 2);
        let checker = unsafe { crate::autd3_client_state_checker(client) };
        assert!(!checker.is_null());

        let inner = &unsafe { &*client }.0;
        assert_eq!(inner.num_devices(), 2);
        let status = unsafe { &*checker }.0.check().unwrap();
        assert_eq!(status.devices(), [DeviceState::Ready; 2]);
        let telemetry = pollster::block_on(inner.read_telemetry()).unwrap();
        assert_eq!(telemetry.len(), 2);

        close_client(client);
        assert!(unsafe { &*checker }.0.check().is_err());
        unsafe { crate::autd3_checker_free(checker) };

        assert_eq!(
            unsafe { autd3_udp_emulator_reboot(emulator, 2) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        unsafe { autd3_udp_emulator_free(emulator) };
    }

    #[test]
    fn a_rejected_client_open_consumes_the_option() {
        let option = autd3_transport_option_new();
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
    fn an_out_of_range_device_count_fails_the_open_through_the_callback() {
        let geometry = Geometry::new(Vec::<Autd3>::new());
        let option = autd3_transport_option_new();
        let config = crate::autd3_client_config_new();
        let (code, client) = complete(|cb, user_data| unsafe {
            crate::autd3_client_open(&raw const geometry, option, config, cb, user_data);
        });
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        assert_eq!(0, client);
        unsafe { crate::autd3_client_config_free(config) };
    }

    type Completion = (i32, usize);

    extern "C" fn report(
        code: i32,
        value: *mut std::ffi::c_void,
        _msg: *const c_char,
        user_data: *mut std::ffi::c_void,
    ) {
        let tx = unsafe { &*user_data.cast::<std::sync::mpsc::Sender<Completion>>() };
        tx.send((code, value as usize)).unwrap();
    }

    fn complete(
        call: impl FnOnce(autd3_ffi_abi::CompletionCallback, *mut std::ffi::c_void),
    ) -> Completion {
        let (tx, rx) = std::sync::mpsc::channel::<Completion>();
        call(Some(report), (&raw const tx).cast_mut().cast());
        rx.recv().unwrap()
    }

    fn send_streaming(
        client: *const crate::ClientHandle,
        command: *mut crate::Pending,
    ) -> Completion {
        complete(|cb, user_data| unsafe {
            crate::autd3_client_send_streaming(client, command, cb, user_data);
        })
    }

    fn await_stream(token: usize) -> i32 {
        complete(|cb, user_data| unsafe {
            crate::autd3_stream_token_await(token as *mut crate::StreamToken, cb, user_data);
        })
        .0
    }

    fn with_client(n: usize, test: impl FnOnce(*const crate::ClientHandle)) {
        let emulator = spawn_emulator(n);
        let client = open_client(emulator, n);

        test(client);

        close_client(client);
        unsafe { autd3_udp_emulator_free(emulator) };
    }

    fn two_nops() -> *mut crate::Pending {
        let ops = [crate::autd3_op_nop(), crate::autd3_op_nop()];
        unsafe { crate::autd3_command_sequence(ops.as_ptr(), ops.len()) }
    }

    #[test]
    fn a_rejected_streaming_send_leaves_the_command_with_the_caller() {
        with_client(1, |client| {
            let command = two_nops();

            let (code, token) = send_streaming(std::ptr::null(), command);
            assert!(code < 0);
            assert_eq!(0, token);

            let (code, token) = send_streaming(client, std::ptr::null_mut());
            assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
            assert_eq!(0, token);

            let (code, token) = send_streaming(client, command);
            assert_eq!(AUTD3_OK, code);
            assert_ne!(0, token);
            assert_eq!(AUTD3_OK, await_stream(token));
        });
    }

    #[test]
    fn a_stream_token_is_consumed_by_its_await_or_freed_unawaited() {
        with_client(1, |client| {
            assert!(await_stream(0) < 0);
            unsafe { crate::autd3_stream_token_free(std::ptr::null_mut()) };

            let (code, token) = send_streaming(client, two_nops());
            assert_eq!(AUTD3_OK, code);
            unsafe { crate::autd3_stream_token_free(token as *mut crate::StreamToken) };

            let (code, token) = send_streaming(client, crate::autd3_op_nop());
            assert_eq!(AUTD3_OK, code);
            assert_eq!(AUTD3_OK, await_stream(token));
        });
    }

    #[test]
    fn a_streaming_send_rejects_an_each_that_does_not_match_the_device_count() {
        with_client(2, |client| {
            let short = [crate::autd3_op_nop()];
            let short = unsafe { crate::autd3_command_each(short.as_ptr(), short.len()) };
            let (code, token) = send_streaming(client, short);
            assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
            assert_eq!(0, token);

            let exact = [crate::autd3_op_nop(), std::ptr::null_mut()];
            let exact = unsafe { crate::autd3_command_each(exact.as_ptr(), exact.len()) };
            let (code, token) = send_streaming(client, exact);
            assert_eq!(AUTD3_OK, code);
            assert_eq!(AUTD3_OK, await_stream(token));
        });
    }
}
