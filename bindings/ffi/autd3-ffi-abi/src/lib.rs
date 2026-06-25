use std::ffi::{CString, c_char, c_void};

use autd3_rs_core::params::NUM_TRANSDUCERS;
use autd3_rs_core::value::Emission;

pub type DevicePattern = [Emission; NUM_TRANSDUCERS];

#[repr(transparent)]
pub struct PatternBuffer(pub Vec<DevicePattern>);

#[repr(transparent)]
pub struct ModulationBuffer(pub Vec<u8>);

#[must_use]
pub fn into_handle<T>(value: T) -> *mut T {
    Box::into_raw(Box::new(value))
}

pub unsafe fn drop_handle<T>(ptr: *mut T) {
    if !ptr.is_null() {
        drop(unsafe { Box::from_raw(ptr) });
    }
}

#[must_use]
pub fn alloc_cstring(s: &str) -> *mut c_char {
    let bytes: Vec<u8> = s.bytes().map(|b| if b == 0 { b' ' } else { b }).collect();

    CString::new(bytes).unwrap_or_default().into_raw()
}

pub unsafe fn free_cstring(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(unsafe { CString::from_raw(ptr) });
    }
}

pub type CompletionCallback =
    extern "C" fn(code: i32, value: *mut c_void, msg: *const c_char, user_data: *mut c_void);

pub struct CompletionCtx {
    cb: CompletionCallback,
    user_data: *mut c_void,
}

unsafe impl Send for CompletionCtx {}

impl CompletionCtx {
    #[must_use]
    pub fn new(cb: CompletionCallback, user_data: *mut c_void) -> Self {
        Self { cb, user_data }
    }

    pub fn ok(self, value: *mut c_void) {
        (self.cb)(0, value, std::ptr::null(), self.user_data);
    }

    pub fn err(self, message: &str) {
        let msg = CString::new(message.replace('\0', " ")).unwrap_or_default();
        (self.cb)(-1, std::ptr::null_mut(), msg.as_ptr(), self.user_data);
    }
}

#[cfg(feature = "client")]
mod client {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;

    use autd3_rs::{ClientConfig, Datagrams};
    use autd3_rs_core::{Error, Geometry};

    pub type BoxFuture<T> = Pin<Box<dyn Future<Output = Result<T, Error>> + Send>>;

    pub struct LinkStatusData {
        pub device_states: Vec<String>,
        pub all_op: bool,
        pub any_lost: bool,
        pub recoveries: u64,
    }

    pub trait ClientBackend: Send + Sync {
        fn num_devices(&self) -> usize;
        fn read_firmware_version(&self) -> BoxFuture<Vec<String>>;
        fn read_fpga_state(&self) -> BoxFuture<Vec<u8>>;
        fn read_error_detail(&self) -> BoxFuture<Vec<u8>>;

        fn send_checked(&self, datagrams: Arc<Datagrams>, frame: Option<usize>) -> BoxFuture<()>;
        fn check_status(&self) -> BoxFuture<LinkStatusData>;
        fn stop(&self) -> BoxFuture<()>;
        fn close(&self) -> BoxFuture<()>;
    }

    pub type ClientOpener =
        Box<dyn FnOnce(Geometry, ClientConfig) -> BoxFuture<Box<dyn ClientBackend>> + Send>;

    pub fn client_opener<F, Fut>(f: F) -> ClientOpener
    where
        F: FnOnce(Geometry, ClientConfig) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Box<dyn ClientBackend>, Error>> + Send + 'static,
    {
        Box::new(move |geo, cfg| Box::pin(f(geo, cfg)))
    }
}

#[cfg(feature = "client")]
pub use client::{BoxFuture, ClientBackend, ClientOpener, LinkStatusData, client_opener};
