use std::ffi::{CString, c_char, c_void};
use std::mem::ManuallyDrop;
use std::ptr::NonNull;

use autd3_rs_core::value::Emission;
use autd3_rs_core::{RtPriority, RtSchedulePolicy};

const fn parse_version_field(s: &str) -> u16 {
    let bytes = s.as_bytes();
    let mut value = 0u16;
    let mut i = 0;
    while i < bytes.len() {
        value = value * 10 + (bytes[i] - b'0') as u16;
        i += 1;
    }
    assert!(value <= 0x3FF, "version field does not fit in 10 bits");
    value
}

pub const AUTD3_ABI_VERSION_MAJOR: u16 = parse_version_field(env!("CARGO_PKG_VERSION_MAJOR"));
pub const AUTD3_ABI_VERSION_MINOR: u16 = parse_version_field(env!("CARGO_PKG_VERSION_MINOR"));
pub const AUTD3_ABI_VERSION_PATCH: u16 = parse_version_field(env!("CARGO_PKG_VERSION_PATCH"));

#[must_use]
pub const fn abi_version() -> u32 {
    ((AUTD3_ABI_VERSION_MAJOR as u32) << 20)
        | ((AUTD3_ABI_VERSION_MINOR as u32) << 10)
        | AUTD3_ABI_VERSION_PATCH as u32
}

#[macro_export]
macro_rules! export_abi_version {
    () => {
        #[unsafe(no_mangle)]
        pub extern "C" fn autd3_abi_version() -> u32 {
            $crate::abi_version()
        }
    };
}

#[macro_export]
macro_rules! option_handle_field {
    ($handle:ty, [$($field:tt).+], duration, $set:ident, $get:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $set(handle: *mut $handle, ns: u64) -> i32 {
            let Some(option) = (unsafe { $crate::handle_mut(handle) }) else {
                return $crate::AUTD3_ERR_INVALID_ARGUMENT;
            };
            option.0.$($field).+ = ::std::time::Duration::from_nanos(ns);
            $crate::AUTD3_OK
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $get(handle: *const $handle, out: *mut u64) -> i32 {
            let Some(option) = (unsafe { $crate::handle_ref(handle) }) else {
                return $crate::AUTD3_ERR_INVALID_ARGUMENT;
            };
            unsafe { $crate::write_out(out, $crate::to_ns(option.0.$($field).+)) }
        }
    };
    ($handle:ty, [$($field:tt).+], $ty:ty, $set:ident, $get:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $set(handle: *mut $handle, value: $ty) -> i32 {
            let Some(option) = (unsafe { $crate::handle_mut(handle) }) else {
                return $crate::AUTD3_ERR_INVALID_ARGUMENT;
            };
            option.0.$($field).+ = value;
            $crate::AUTD3_OK
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $get(handle: *const $handle, out: *mut $ty) -> i32 {
            let Some(option) = (unsafe { $crate::handle_ref(handle) }) else {
                return $crate::AUTD3_ERR_INVALID_ARGUMENT;
            };
            unsafe { $crate::write_out(out, option.0.$($field).+) }
        }
    };
}

#[macro_export]
macro_rules! option_handle_opt_duration_field {
    ($handle:ty, [$($field:tt).+], $set:ident, $get:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $set(handle: *mut $handle, has_value: bool, ns: u64) -> i32 {
            let Some(option) = (unsafe { $crate::handle_mut(handle) }) else {
                return $crate::AUTD3_ERR_INVALID_ARGUMENT;
            };
            option.0.$($field).+ = has_value.then(|| ::std::time::Duration::from_nanos(ns));
            $crate::AUTD3_OK
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $get(
            handle: *const $handle,
            out_has_value: *mut bool,
            out_ns: *mut u64,
        ) -> i32 {
            let Some(option) = (unsafe { $crate::handle_ref(handle) }) else {
                return $crate::AUTD3_ERR_INVALID_ARGUMENT;
            };
            let value = option.0.$($field).+;
            let code = unsafe { $crate::write_out(out_has_value, value.is_some()) };
            if code != $crate::AUTD3_OK {
                return code;
            }
            unsafe { $crate::write_out(out_ns, $crate::to_ns(value.unwrap_or_default())) }
        }
    };
}

#[macro_export]
macro_rules! option_handle_iface {
    ($handle:ty, [$($field:tt).+], $set:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $set(
            handle: *mut $handle,
            iface: *const ::std::ffi::c_char,
        ) -> i32 {
            let Some(option) = (unsafe { $crate::handle_mut(handle) }) else {
                return $crate::AUTD3_ERR_INVALID_ARGUMENT;
            };
            option.0.$($field).+ =
                ::autd3_rs_core::Interface::from(unsafe { $crate::cstr_to_string(iface) });
            $crate::AUTD3_OK
        }
    };
}

#[macro_export]
macro_rules! option_handle_lifecycle {
    ($handle:ty, $free:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $free(handle: *mut $handle) {
            unsafe { $crate::drop_handle(handle) }
        }
    };
}

pub const AUTD3_OK: i32 = 0;
pub const AUTD3_ERR: i32 = -1;
pub const AUTD3_ERR_TIMEOUT: i32 = -2;
pub const AUTD3_ERR_DEVICE: i32 = -3;
pub const AUTD3_ERR_LINK: i32 = -4;
pub const AUTD3_ERR_INVALID_ARGUMENT: i32 = -5;
pub const AUTD3_ERR_UNSUPPORTED_FIRMWARE: i32 = -6;
pub const AUTD3_ERR_ABORTED: i32 = -7;

pub const OPTION_HANDLE_CONSUMED: &str =
    "link option handle is null; it was already consumed by a previous open call";

pub const AUTD3_RT_PRIORITY_DEFAULT: u8 = 0;
pub const AUTD3_RT_PRIORITY_DISABLED: u8 = 1;
pub const AUTD3_RT_PRIORITY_EXPLICIT: u8 = 2;

#[must_use]
pub fn to_rt_priority(mode: u8, value: u8) -> Option<Option<RtPriority>> {
    match mode {
        AUTD3_RT_PRIORITY_DEFAULT => Some(autd3_rs_core::default_rt_priority()),
        AUTD3_RT_PRIORITY_DISABLED => Some(None),
        AUTD3_RT_PRIORITY_EXPLICIT => RtPriority::new(value).map(Some),
        _ => None,
    }
}

#[must_use]
pub fn to_rt_policy(value: u8) -> Option<RtSchedulePolicy> {
    match value {
        0 => Some(RtSchedulePolicy::Normal),
        1 => Some(RtSchedulePolicy::Fifo),
        2 => Some(RtSchedulePolicy::RoundRobin),
        _ => None,
    }
}

#[must_use]
pub fn from_rt_policy(policy: RtSchedulePolicy) -> u8 {
    match policy {
        RtSchedulePolicy::Normal => 0,
        RtSchedulePolicy::RoundRobin => 2,
        _ => 1,
    }
}

pub type DevicePattern = Vec<Emission>;

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

pub unsafe fn take_handle<T>(ptr: *mut T) -> Option<T> {
    let ptr = NonNull::new(ptr)?;
    Some(*unsafe { Box::from_raw(ptr.as_ptr()) })
}

pub unsafe fn handle_ref<'a, T>(ptr: *const T) -> Option<&'a T> {
    unsafe { ptr.as_ref() }
}

pub unsafe fn handle_mut<'a, T>(ptr: *mut T) -> Option<&'a mut T> {
    unsafe { ptr.as_mut() }
}

pub unsafe fn slice_ref<'a, T>(ptr: *const T, len: usize) -> Option<&'a [T]> {
    if len == 0 {
        return Some(&[]);
    }
    let ptr = NonNull::new(ptr.cast_mut())?;
    Some(unsafe { std::slice::from_raw_parts(ptr.as_ptr().cast_const(), len) })
}

pub unsafe fn slice_mut<'a, T>(ptr: *mut T, len: usize) -> Option<&'a mut [T]> {
    if len == 0 {
        return Some(&mut []);
    }
    let ptr = NonNull::new(ptr)?;
    Some(unsafe { std::slice::from_raw_parts_mut(ptr.as_ptr(), len) })
}

#[must_use]
pub unsafe fn cstr_to_string(ptr: *const c_char) -> Option<String> {
    let ptr = NonNull::new(ptr.cast_mut())?;
    Some(
        unsafe { std::ffi::CStr::from_ptr(ptr.as_ptr().cast_const()) }
            .to_string_lossy()
            .into_owned(),
    )
}

pub unsafe fn write_cstr(buf: *mut c_char, len: usize, s: &str) {
    let Some(buf) = NonNull::new(buf) else {
        return;
    };
    if len == 0 {
        return;
    }
    let bytes = s.as_bytes();
    let n = bytes.len().min(len - 1);

    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr().cast::<c_char>(), buf.as_ptr(), n);
        *buf.as_ptr().add(n) = 0;
    }
}

pub unsafe fn write_out<T>(ptr: *mut T, value: T) -> i32 {
    let Some(ptr) = NonNull::new(ptr) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe { ptr.as_ptr().write(value) };
    AUTD3_OK
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

pub type CompletionFn =
    extern "C" fn(code: i32, value: *mut c_void, msg: *const c_char, user_data: *mut c_void);

pub type CompletionCallback = Option<CompletionFn>;

pub struct CompletionCtx {
    cb: CompletionFn,
    user_data: *mut c_void,
}

unsafe impl Send for CompletionCtx {}

impl CompletionCtx {
    #[must_use]
    pub fn new(cb: CompletionCallback, user_data: *mut c_void) -> Option<Self> {
        Some(Self { cb: cb?, user_data })
    }

    pub fn ok(self, value: *mut c_void) {
        let this = ManuallyDrop::new(self);
        (this.cb)(AUTD3_OK, value, std::ptr::null(), this.user_data);
    }

    pub fn err(self, message: &str) {
        self.fail(AUTD3_ERR, message);
    }

    pub fn invalid_argument(self, message: &str) {
        self.fail(AUTD3_ERR_INVALID_ARGUMENT, message);
    }

    pub fn fail(self, code: i32, message: &str) {
        let this = ManuallyDrop::new(self);
        let msg = CString::new(message.replace('\0', " ")).unwrap_or_default();
        (this.cb)(code, std::ptr::null_mut(), msg.as_ptr(), this.user_data);
    }
}

impl Drop for CompletionCtx {
    fn drop(&mut self) {
        let msg = c"operation aborted";
        (self.cb)(
            AUTD3_ERR_ABORTED,
            std::ptr::null_mut(),
            msg.as_ptr(),
            self.user_data,
        );
    }
}

#[cfg(feature = "client")]
mod client {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::time::Duration;

    use autd3_rs::Error;
    use autd3_rs::{ClientConfig, Frames, Response, ResponseFuture, Telemetry};
    use autd3_rs_core::Geometry;
    use autd3_rs_core::link::DeviceState;

    use super::{
        AUTD3_ERR, AUTD3_ERR_DEVICE, AUTD3_ERR_INVALID_ARGUMENT, AUTD3_ERR_LINK, AUTD3_ERR_TIMEOUT,
        AUTD3_ERR_UNSUPPORTED_FIRMWARE, CompletionCtx,
    };

    pub trait ErrorCategory: std::fmt::Display {
        fn error_code(&self) -> i32;
    }

    impl ErrorCategory for Error {
        fn error_code(&self) -> i32 {
            match self {
                Error::Timeout { .. } => AUTD3_ERR_TIMEOUT,
                Error::DeviceError { .. } => AUTD3_ERR_DEVICE,
                Error::Link(_) => AUTD3_ERR_LINK,
                Error::UnsupportedFirmware { .. } => AUTD3_ERR_UNSUPPORTED_FIRMWARE,
                Error::SilencerConstraint { .. }
                | Error::TransitionConstraint { .. }
                | Error::InvalidPayload(_)
                | Error::Encode(_) => AUTD3_ERR_INVALID_ARGUMENT,
                _ => AUTD3_ERR,
            }
        }
    }

    impl ErrorCategory for LegacyError {
        fn error_code(&self) -> i32 {
            match self {
                LegacyError::Timeout { .. } | LegacyError::BusNotOperational { .. } => {
                    AUTD3_ERR_TIMEOUT
                }
                LegacyError::Device { .. } | LegacyError::FpgaStateInvalid { .. } => {
                    AUTD3_ERR_DEVICE
                }
                LegacyError::Link(_) => AUTD3_ERR_LINK,
                LegacyError::UnsupportedFirmware { .. } => AUTD3_ERR_UNSUPPORTED_FIRMWARE,
                LegacyError::DeviceCountMismatch { .. }
                | LegacyError::NoDevices
                | LegacyError::Encode(_)
                | LegacyError::SamplingConfig(_)
                | LegacyError::PulseWidth(_)
                | LegacyError::InvalidPayload(_) => AUTD3_ERR_INVALID_ARGUMENT,
                _ => AUTD3_ERR,
            }
        }
    }

    impl CompletionCtx {
        pub fn err_of<E: ErrorCategory>(self, e: &E) {
            self.fail(e.error_code(), &e.to_string());
        }
    }

    #[must_use]
    pub fn link_err(message: impl Into<String>) -> Error {
        autd3_rs_core::error::LinkError::new(message).into()
    }

    #[must_use]
    pub fn to_ns(d: Duration) -> u64 {
        u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)
    }

    pub type BoxFuture<T> = Pin<Box<dyn Future<Output = Result<T, Error>> + Send>>;

    pub struct LinkStatusData {
        pub devices: Vec<DeviceState>,
        pub recoveries: u64,
    }

    pub fn merge_response(merged: &mut [u8], response: &[u8]) {
        merged
            .iter_mut()
            .zip(response.iter().copied())
            .for_each(|(m, d)| {
                if *m == 0 {
                    *m = d;
                }
            });
    }

    pub struct ResponseTokenData(pub BoxFuture<Response>);

    impl ResponseTokenData {
        #[must_use]
        pub fn from_futures(futures: Vec<ResponseFuture>) -> Self {
            Self(Box::pin(async move {
                let mut merged: Option<Response> = None;
                for future in futures {
                    let response = future.await?;
                    match merged.as_mut() {
                        None => merged = Some(response),
                        Some(m) => m.merge(&response),
                    }
                }
                Ok(merged.unwrap_or_default())
            }))
        }
    }

    pub trait CheckerBackend: Send + Sync {
        fn check(&self) -> Result<LinkStatusData, Error>;
    }

    pub trait ClientBackend: Send + Sync {
        fn num_devices(&self) -> usize;
        fn dc_offset_ns(&self) -> i64;
        fn read_firmware_version(&self) -> BoxFuture<Vec<String>>;
        fn read_fpga_state(&self) -> BoxFuture<Vec<u8>>;
        fn read_error_detail(&self) -> BoxFuture<Vec<u8>>;
        fn read_telemetry(&self, counter: Telemetry) -> BoxFuture<Vec<u8>>;

        fn send(
            &self,
            datagrams: Arc<Frames>,
            frame: Option<usize>,
        ) -> BoxFuture<ResponseTokenData>;
        fn send_checked(&self, datagrams: Arc<Frames>, frame: Option<usize>) -> BoxFuture<()>;
        fn checker(&self) -> Box<dyn CheckerBackend>;
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

    use autd3_rs::legacy::{LegacyClient, LegacyClientConfig, LegacyError, LegacyFrames};
    use autd3_rs_core::link::{IntoLink, StateCheck};

    pub type LegacyBoxFuture<T> = Pin<Box<dyn Future<Output = Result<T, LegacyError>> + Send>>;

    pub trait LegacyClientBackend: Send + Sync {
        fn num_devices(&self) -> usize;
        fn dc_offset_ns(&self) -> i64;
        fn read_firmware_version(&self) -> LegacyBoxFuture<Vec<String>>;
        fn read_fpga_state(&self) -> LegacyBoxFuture<Vec<u8>>;
        fn send(&self, frames: Arc<LegacyFrames>, frame: Option<usize>)
        -> LegacyBoxFuture<Vec<u8>>;
        fn send_checked(
            &self,
            frames: Arc<LegacyFrames>,
            frame: Option<usize>,
        ) -> LegacyBoxFuture<()>;
        fn checker(&self) -> Box<dyn CheckerBackend>;
        fn stop(&self) -> LegacyBoxFuture<()>;
        fn close(&self) -> LegacyBoxFuture<()>;
    }

    pub type LegacyClientOpener = Box<
        dyn FnOnce(Geometry, LegacyClientConfig) -> LegacyBoxFuture<Box<dyn LegacyClientBackend>>
            + Send,
    >;

    struct LegacyBackend<C> {
        client: Arc<LegacyClient>,
        checker: Arc<std::sync::Mutex<C>>,
    }

    struct LegacyChecker<C>(Arc<std::sync::Mutex<C>>);

    impl<C: StateCheck> CheckerBackend for LegacyChecker<C> {
        fn check(&self) -> Result<LinkStatusData, Error> {
            let status = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .check()
                .map_err(|e| link_err(e.to_string()))?;
            Ok(LinkStatusData {
                devices: status.devices().to_vec(),
                recoveries: status.recoveries(),
            })
        }
    }

    fn frame_range(
        frames: &LegacyFrames,
        frame: Option<usize>,
    ) -> Result<(usize, usize), LegacyError> {
        match frame {
            Some(index) if index >= frames.len() => {
                Err(LegacyError::Link(format!("frame {index} out of range")))
            }
            Some(index) => Ok((index, index + 1)),
            None => Ok((0, frames.len())),
        }
    }

    impl<C: StateCheck> LegacyClientBackend for LegacyBackend<C> {
        fn num_devices(&self) -> usize {
            self.client.num_devices()
        }

        fn dc_offset_ns(&self) -> i64 {
            self.client.dc_offset_ns()
        }

        fn read_firmware_version(&self) -> LegacyBoxFuture<Vec<String>> {
            let client = Arc::clone(&self.client);
            Box::pin(async move {
                let versions = client.read_firmware_version().await?;
                Ok::<Vec<String>, LegacyError>(versions.iter().map(ToString::to_string).collect())
            })
        }

        fn read_fpga_state(&self) -> LegacyBoxFuture<Vec<u8>> {
            let client = Arc::clone(&self.client);
            Box::pin(async move {
                let states = client.read_fpga_state().await?;
                Ok::<Vec<u8>, LegacyError>(states.iter().map(|s| s.0).collect())
            })
        }

        fn send(
            &self,
            frames: Arc<LegacyFrames>,
            frame: Option<usize>,
        ) -> LegacyBoxFuture<Vec<u8>> {
            let client = Arc::clone(&self.client);
            Box::pin(async move {
                let (start, end) = frame_range(&frames, frame)?;
                let mut merged: Option<Vec<u8>> = None;
                for index in start..end {
                    let frame = frames
                        .frame(index)
                        .ok_or_else(|| LegacyError::Link(format!("frame {index} out of range")))?;
                    let data = client.send(frame).await?.data().to_vec();
                    match merged.as_mut() {
                        None => merged = Some(data),
                        Some(m) => merge_response(m, &data),
                    }
                }
                Ok::<Vec<u8>, LegacyError>(merged.unwrap_or_default())
            })
        }

        fn send_checked(
            &self,
            frames: Arc<LegacyFrames>,
            frame: Option<usize>,
        ) -> LegacyBoxFuture<()> {
            let client = Arc::clone(&self.client);
            Box::pin(async move {
                let (start, end) = frame_range(&frames, frame)?;
                for index in start..end {
                    let frame = frames
                        .frame(index)
                        .ok_or_else(|| LegacyError::Link(format!("frame {index} out of range")))?;
                    client.send_checked(frame).await?;
                }
                Ok::<(), LegacyError>(())
            })
        }

        fn checker(&self) -> Box<dyn CheckerBackend> {
            Box::new(LegacyChecker(Arc::clone(&self.checker)))
        }

        fn stop(&self) -> LegacyBoxFuture<()> {
            let client = Arc::clone(&self.client);
            Box::pin(async move { client.stop().await })
        }

        fn close(&self) -> LegacyBoxFuture<()> {
            let client = Arc::clone(&self.client);
            Box::pin(async move { client.close().await })
        }
    }

    pub fn legacy_client_opener<T, F>(make_link: F) -> LegacyClientOpener
    where
        F: FnOnce(&Geometry) -> Result<T, LegacyError> + Send + 'static,
        T: IntoLink + 'static,
    {
        Box::new(move |geometry, config| {
            Box::pin(async move {
                let link = make_link(&geometry)?;
                let (client, checker) =
                    LegacyClient::open_with_checker(&geometry, link, config).await?;
                let backend: Box<dyn LegacyClientBackend> = Box::new(LegacyBackend {
                    client: Arc::new(client),
                    checker: Arc::new(std::sync::Mutex::new(checker)),
                });
                Ok(backend)
            })
        })
    }

    #[cfg(test)]
    mod tests {
        use super::merge_response;

        fn merge_all(responses: &[&[u8]]) -> Vec<u8> {
            let mut merged: Option<Vec<u8>> = None;
            for response in responses {
                match merged.as_mut() {
                    None => merged = Some(response.to_vec()),
                    Some(m) => merge_response(m, response),
                }
            }
            merged.unwrap_or_default()
        }

        #[test]
        fn every_frame_contributes_to_the_merged_response() {
            assert_eq!(
                vec![1, 2, 3],
                merge_all(&[&[1, 0, 0], &[0, 2, 0], &[0, 0, 3]])
            );
        }

        #[test]
        fn the_first_non_zero_byte_wins() {
            assert_eq!(vec![1, 5], merge_all(&[&[0, 5], &[1, 6], &[2, 7]]));
        }

        #[test]
        fn a_shorter_response_leaves_the_tail_untouched() {
            assert_eq!(vec![1, 2, 0], merge_all(&[&[0, 0, 0], &[1, 2]]));
        }

        #[test]
        fn no_frame_merges_to_an_empty_response() {
            assert_eq!(Vec::<u8>::new(), merge_all(&[]));
        }
    }
}

#[cfg(feature = "client")]
pub use client::{
    BoxFuture, CheckerBackend, ClientBackend, ClientOpener, ErrorCategory, LegacyBoxFuture,
    LegacyClientBackend, LegacyClientOpener, LinkStatusData, ResponseTokenData, client_opener,
    legacy_client_opener, link_err, to_ns,
};
