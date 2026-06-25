mod error;
mod link;
mod server;
mod wire;

pub use error::RemoteLinkError;
pub use link::{RemoteLink, RemoteLinkOption};
pub use server::RemoteServer;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransducerLayout {
    pub pos: [f32; 3],
    pub dir: [f32; 3],
}
