use std::io;
use std::time::Duration;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::{RawSocket, interface_candidates};
#[cfg(target_os = "macos")]
pub use macos::{RawSocket, interface_candidates};
#[cfg(target_os = "windows")]
pub use windows::{RawSocket, interface_candidates};

pub trait RawBus: Send + 'static {
    fn send(&mut self, frame: &[u8]) -> io::Result<()>;

    fn receive(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>>;

    fn mtu(&self) -> usize;

    fn echoes_sent_frames(&self) -> bool {
        false
    }
}
