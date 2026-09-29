use std::io;
use std::time::Duration;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::{PERMISSION_HINT, RawSocket, available, interface_candidates};
#[cfg(target_os = "macos")]
pub use macos::{PERMISSION_HINT, RawSocket, available, interface_candidates};
#[cfg(target_os = "windows")]
pub use windows::{PERMISSION_HINT, RawSocket, available, interface_candidates};

pub trait RawBus: Send {
    fn send(&mut self, frame: &[u8]) -> io::Result<()>;

    fn receive(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>>;

    fn mtu(&self) -> usize;
}

impl<B: RawBus + ?Sized> RawBus for Box<B> {
    fn send(&mut self, frame: &[u8]) -> io::Result<()> {
        (**self).send(frame)
    }

    fn receive(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
        (**self).receive(buf, timeout)
    }

    fn mtu(&self) -> usize {
        (**self).mtu()
    }
}
