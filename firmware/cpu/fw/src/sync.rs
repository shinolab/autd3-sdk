#[cfg(not(loom))]
pub(crate) use core::sync::atomic::{AtomicU16, Ordering};
#[cfg(loom)]
pub(crate) use loom::sync::atomic::{AtomicU16, Ordering};
