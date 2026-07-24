#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(target_os = "windows")]
pub(crate) mod windows;

pub(crate) mod thread;
pub(crate) mod timer;
pub(crate) mod worker;
