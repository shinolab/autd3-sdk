use std::ffi::c_char;

use autd3_binding_rt::{LogWriter, TracingGuard, TracingOption, try_init_tracing};
use autd3_ffi_abi::{
    AUTD3_ERR_INVALID_ARGUMENT, AUTD3_OK, cstr_to_string, drop_handle, into_handle, write_cstr,
    write_out,
};

pub(crate) const AUTD3_LOG_WRITER_STDOUT: u8 = 0;
pub(crate) const AUTD3_LOG_WRITER_STDERR: u8 = 1;

pub(crate) struct TracingGuardHandle(#[expect(dead_code)] TracingGuard);

#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn autd3_tracing_option_default(
    out_default_filter: *mut c_char,
    out_default_filter_len: usize,
    out_writer: *mut u8,
) -> i32 {
    let default = TracingOption::default();
    if out_default_filter.is_null() || out_default_filter_len <= default.default_filter.len() {
        return AUTD3_ERR_INVALID_ARGUMENT;
    }
    let writer = match default.writer {
        LogWriter::Stdout => AUTD3_LOG_WRITER_STDOUT,
        LogWriter::Stderr => AUTD3_LOG_WRITER_STDERR,
        _ => unreachable!(),
    };
    let code = unsafe { write_out(out_writer, writer) };
    if code != AUTD3_OK {
        return code;
    }
    unsafe {
        write_cstr(
            out_default_filter,
            out_default_filter_len,
            default.default_filter,
        );
    };
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn autd3_init_tracing(
    default_filter: *const c_char,
    writer: u8,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut TracingGuardHandle {
    let Some(default_filter) = (unsafe { cstr_to_string(default_filter) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null or non-UTF-8 default filter") };
        return std::ptr::null_mut();
    };
    let writer = match writer {
        AUTD3_LOG_WRITER_STDOUT => LogWriter::Stdout,
        AUTD3_LOG_WRITER_STDERR => LogWriter::Stderr,
        _ => {
            unsafe { write_cstr(out_err, out_err_len, "unknown log writer") };
            return std::ptr::null_mut();
        }
    };

    match try_init_tracing(TracingOption {
        default_filter: default_filter.leak(),
        writer,
    }) {
        Ok(guard) => into_handle(TracingGuardHandle(guard)),
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn autd3_tracing_guard_free(guard: *mut TracingGuardHandle) {
    unsafe { drop_handle(guard) }
}

#[cfg(test)]
mod tests {
    use std::ffi::{CStr, CString};

    use super::*;

    fn message(err: &[c_char]) -> String {
        unsafe { CStr::from_ptr(err.as_ptr()) }
            .to_str()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn the_default_option_matches_rust() {
        let mut filter = [0 as c_char; 32];
        let mut writer = u8::MAX;

        let code = unsafe {
            autd3_tracing_option_default(filter.as_mut_ptr(), filter.len(), &raw mut writer)
        };

        assert_eq!(AUTD3_OK, code);
        assert_eq!(TracingOption::default().default_filter, message(&filter));
        assert_eq!(AUTD3_LOG_WRITER_STDOUT, writer);
    }

    #[test]
    fn the_default_option_rejects_a_buffer_that_cannot_hold_the_filter() {
        let mut filter = [0 as c_char; 4];
        let mut writer = 0;

        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_tracing_option_default(filter.as_mut_ptr(), filter.len(), &raw mut writer)
        });
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_tracing_option_default(std::ptr::null_mut(), 0, &raw mut writer)
        });
    }

    #[test]
    fn invalid_arguments_are_rejected_before_installing_anything() {
        let filter = CString::new("info").unwrap();
        let mut err = [0 as c_char; 128];

        let guard = unsafe { autd3_init_tracing(std::ptr::null(), 0, err.as_mut_ptr(), err.len()) };
        assert!(guard.is_null());
        assert_eq!("null or non-UTF-8 default filter", message(&err));

        let guard = unsafe { autd3_init_tracing(filter.as_ptr(), 2, err.as_mut_ptr(), err.len()) };
        assert!(guard.is_null());
        assert_eq!("unknown log writer", message(&err));
    }

    #[test]
    fn a_second_initialization_reports_an_error_instead_of_aborting() {
        let filter = CString::new("off").unwrap();
        let mut err = [0 as c_char; 256];

        let first = unsafe {
            autd3_init_tracing(
                filter.as_ptr(),
                AUTD3_LOG_WRITER_STDERR,
                err.as_mut_ptr(),
                err.len(),
            )
        };
        let second = unsafe {
            autd3_init_tracing(
                filter.as_ptr(),
                AUTD3_LOG_WRITER_STDERR,
                err.as_mut_ptr(),
                err.len(),
            )
        };

        assert!(second.is_null());
        assert!(message(&err).contains("tracing subscriber"));
        unsafe { autd3_tracing_guard_free(first) };
        unsafe { autd3_tracing_guard_free(second) };
    }
}
