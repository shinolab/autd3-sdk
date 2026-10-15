pub(crate) mod conn;
pub mod message;

use std::net::SocketAddr;
use std::time::Duration;

use crate::udp::UdpOption;

pub use message::MessageError;

pub const SERVER_PORT: u16 = 44338;

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Link {
    Udp(UdpOption),
    Server(ServerLink),
}

impl Default for Link {
    fn default() -> Self {
        Link::Udp(UdpOption::default())
    }
}

impl From<UdpOption> for Link {
    fn from(option: UdpOption) -> Self {
        Link::Udp(option)
    }
}

impl From<ServerLink> for Link {
    fn from(server: ServerLink) -> Self {
        Link::Server(server)
    }
}

impl From<&UdpOption> for Link {
    fn from(option: &UdpOption) -> Self {
        Link::Udp(option.clone())
    }
}

impl From<&ServerLink> for Link {
    fn from(server: &ServerLink) -> Self {
        Link::Server(*server)
    }
}

impl From<&Link> for Link {
    fn from(link: &Link) -> Self {
        link.clone()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerLink {
    pub port: u16,
    pub timeout: Duration,
}

impl Default for ServerLink {
    fn default() -> Self {
        Self {
            port: SERVER_PORT,
            timeout: Duration::from_secs(1),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LinkError {
    #[error("no server accepts a connection at {0}")]
    ServerNotFound(SocketAddr),
    #[error("the server refused the client: {0}")]
    Rejected(String),
    #[error("the server speaks link version {server}, but this client speaks {client}")]
    UnsupportedVersion { server: u16, client: u16 },
    #[error("the server announced {server} devices, but the geometry has {client}")]
    DeviceCountMismatch { server: usize, client: usize },
    #[error("the server did not answer within {0:?}")]
    Timeout(Duration),
    #[error("the timeout of the server link must not be zero")]
    ZeroTimeout,
    #[error("the server closed the connection")]
    Disconnected,
    #[error("the server stopped serving the devices: {0}")]
    ServerFailed(String),
    #[error("the server could not send a frame: {0}")]
    SendFailed(String),
    #[error("the server sent an unexpected message: {0}")]
    Unexpected(&'static str),
    #[error(transparent)]
    Message(#[from] MessageError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_link_talks_udp_and_the_server_link_defaults_to_the_well_known_port() {
        assert_eq!(Link::default(), Link::Udp(UdpOption::default()));
        assert_eq!(
            ServerLink::default(),
            ServerLink {
                port: 44338,
                timeout: Duration::from_secs(1),
            }
        );
        assert_eq!(
            Link::from(ServerLink::default()),
            Link::Server(ServerLink::default())
        );
        assert_eq!(
            Link::from(&ServerLink::default()),
            Link::from(&Link::Server(ServerLink::default()))
        );
        assert_eq!(Link::from(&UdpOption::default()), Link::default());
    }
}
