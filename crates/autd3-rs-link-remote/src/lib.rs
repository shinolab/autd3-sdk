mod bus;
#[cfg(feature = "discovery")]
mod discovery;
mod error;
mod link;
mod server;
mod wire;

pub const WIRE_VERSION: u8 = wire::VERSION;

pub use bus::{Actual, BusOption, BusPacing, BusSnapshot, Desired, SharedBus};
#[cfg(feature = "discovery")]
pub use discovery::{
    Advertisement, AdvertisementHandle, Appliance, DiscoveryError, DiscoveryOption, SERVICE_TYPE,
    TXT_CONTROL_PORT, TXT_SDK_VERSION, TXT_WIRE_VERSION, advertise, discover, discover_all,
    instance_name,
};
pub use error::{PeerVersion, RejectKind, RemoteLinkError};
pub use link::{RemoteLink, RemoteLinkOption, RemoteStateChecker};
pub use server::{BusServer, BusServerOption, RemoteServer, RemoteServerOption, Session, Sessions};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransducerLayout {
    pub pos: [f32; 3],
    pub dir: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceLayout {
    pub transducers: Vec<TransducerLayout>,
}
