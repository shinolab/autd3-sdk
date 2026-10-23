use std::ffi::c_char;
use std::net::SocketAddrV6;
use std::num::NonZeroUsize;

use autd3_ffi_abi::{
    AUTD3_ERR_INVALID_ARGUMENT, AUTD3_OK, alloc_cstring, cstr_to_string, free_cstring, handle_mut,
    handle_ref, into_handle, write_out,
};
use autd3_rs::Interface;
use autd3_rs::udp::TransportOption as CoreOption;

pub struct TransportOptionHandle(pub(crate) CoreOption);

#[unsafe(no_mangle)]
pub(crate) extern "C" fn autd3_transport_option_new() -> *mut TransportOptionHandle {
    into_handle(TransportOptionHandle(CoreOption::default()))
}

#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn autd3_transport_option_set_send_buffer_bytes(
    handle: *mut TransportOptionHandle,
    bytes: u64,
) -> i32 {
    let Some(option) = (unsafe { autd3_ffi_abi::handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let Ok(bytes) = usize::try_from(bytes) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    option.0.send_buffer_bytes = NonZeroUsize::new(bytes);
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn autd3_transport_option_get_send_buffer_bytes(
    handle: *const TransportOptionHandle,
    out: *mut u64,
) -> i32 {
    let Some(option) = (unsafe { autd3_ffi_abi::handle_ref(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe {
        autd3_ffi_abi::write_out(
            out,
            option
                .0
                .send_buffer_bytes
                .map_or(0, |bytes| bytes.get() as u64),
        )
    }
}
autd3_ffi_abi::option_handle_field!(
    TransportOptionHandle,
    [lost_timeout],
    duration,
    autd3_transport_option_set_lost_timeout,
    autd3_transport_option_get_lost_timeout
);
autd3_ffi_abi::option_handle_field!(
    TransportOptionHandle,
    [open_timeout],
    duration,
    autd3_transport_option_set_open_timeout,
    autd3_transport_option_get_open_timeout
);
autd3_ffi_abi::option_handle_field!(
    TransportOptionHandle,
    [ack_timeout],
    duration,
    autd3_transport_option_set_ack_timeout,
    autd3_transport_option_get_ack_timeout
);
autd3_ffi_abi::option_handle_lifecycle!(TransportOptionHandle, autd3_transport_option_free);

#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn autd3_transport_option_set_iface(
    handle: *mut TransportOptionHandle,
    iface: *const c_char,
) -> i32 {
    let Some(option) = (unsafe { handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    option.0.iface = unsafe { cstr_to_string(iface) }.map_or(Interface::Auto, Interface::Name);
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn autd3_transport_option_set_iface_addr(
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
pub(crate) unsafe extern "C" fn autd3_transport_option_get_iface_addr(
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
pub(crate) unsafe extern "C" fn autd3_udp_free_string(ptr: *mut c_char) {
    unsafe { free_cstring(ptr) };
}

#[cfg(test)]
mod tests {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::ffi::CString;
    use std::time::Duration;

    use autd3_rs::Geometry;
    use autd3_rs::geometry::Autd3;

    use super::*;

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
        assert_eq!(lost_timeout(handle), option.lost_timeout);
        let mut ns = 0u64;
        assert_eq!(
            unsafe { autd3_transport_option_get_open_timeout(handle, &raw mut ns) },
            AUTD3_OK
        );
        assert_eq!(Duration::from_nanos(ns), option.open_timeout);
        assert_eq!(
            unsafe { autd3_transport_option_get_ack_timeout(handle, &raw mut ns) },
            AUTD3_OK
        );
        assert_eq!(Duration::from_nanos(ns), option.ack_timeout);
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
    fn a_null_handle_is_an_argument_error_not_a_crash() {
        let mut ns = 0u64;
        assert_eq!(
            unsafe { autd3_transport_option_get_lost_timeout(std::ptr::null(), &raw mut ns) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
    }

    fn open(
        geometry: *const Geometry,
        option: *mut TransportOptionHandle,
    ) -> (i32, *mut crate::ClientHandle, String) {
        let mut client = std::ptr::NonNull::<crate::ClientHandle>::dangling().as_ptr();
        let mut err = [0 as c_char; 256];
        let code = unsafe {
            crate::autd3_client_open(
                geometry,
                option,
                &raw mut client,
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
    fn a_rejected_client_open_consumes_the_option() {
        let option = autd3_transport_option_new();
        let (code, client, message) = open(std::ptr::null(), option);
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        assert!(client.is_null());
        assert_eq!("null argument", message);

        let geometry = Geometry::new(vec![Autd3::default()]);
        let (code, client, _) = open(&raw const geometry, std::ptr::null_mut());
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        assert!(client.is_null());
    }

    #[test]
    fn an_out_of_range_device_count_fails_the_open() {
        let geometry = Geometry::new(Vec::<Autd3>::new());
        let option = autd3_transport_option_new();
        let (code, client, message) = open(&raw const geometry, option);
        assert_eq!(crate::AUTD3_ERR_NETWORK, code);
        assert!(client.is_null());
        assert_eq!(
            "network error: the number of devices must be 1..=128, but 0 was given",
            message
        );
    }

    #[test]
    fn a_client_open_without_an_out_pointer_still_consumes_the_option() {
        let geometry = Geometry::new(vec![Autd3::default()]);
        let live = LIVE_ALLOCATIONS.get();
        let option = autd3_transport_option_new();
        assert!(live < LIVE_ALLOCATIONS.get());
        let code = unsafe {
            crate::autd3_client_open(
                &raw const geometry,
                option,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        };
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        assert_eq!(live, LIVE_ALLOCATIONS.get());
    }

    #[test]
    fn a_token_await_without_a_callback_still_consumes_the_token() {
        let live = LIVE_ALLOCATIONS.get();
        let token = into_handle(crate::ResponseToken(Vec::new()));
        assert!(live < LIVE_ALLOCATIONS.get());
        unsafe { crate::autd3_response_token_await(token, None, std::ptr::null_mut()) };
        assert_eq!(live, LIVE_ALLOCATIONS.get());
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
}
