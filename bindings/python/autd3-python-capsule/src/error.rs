use pyo3::prelude::*;

fn new_error(class: &str, msg: String) -> PyErr {
    Python::attach(|py| {
        match py
            .import("autd3_core")
            .and_then(|m| m.getattr(class))
            .and_then(|c| c.call1((msg,)))
        {
            Ok(err) => PyErr::from_value(err),
            Err(err) => err,
        }
    })
}

pub fn generic<E: core::fmt::Display>(e: E) -> PyErr {
    new_error("Autd3Error", e.to_string())
}

pub fn invalid_argument<E: core::fmt::Display>(e: E) -> PyErr {
    new_error("InvalidArgumentError", e.to_string())
}

#[cfg(feature = "frame")]
#[allow(clippy::needless_pass_by_value)]
#[must_use]
pub fn client(e: autd3_rs::Error) -> PyErr {
    use autd3_rs::Error;
    let class = match &e {
        Error::Timeout { .. } => "ReplyTimeoutError",
        Error::DeviceError { .. } | Error::UnexpectedReply { .. } => "DeviceError",
        Error::Network(_) | Error::DeviceLost { .. } => "NetworkError",
        Error::InvalidPayload(_) => "InvalidArgumentError",
        Error::DeviceTimeUnknown | Error::Closed => "Autd3Error",
        _ => unreachable!(),
    };
    new_error(class, e.to_string())
}
