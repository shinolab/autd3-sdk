mod client;
mod error;
#[cfg(feature = "discovery")]
mod mdns;
mod server;
mod wire;

pub const WIRE_VERSION: u8 = wire::VERSION;

#[cfg(feature = "discovery")]
pub use client::{Appliance, DiscoveryOption, discover, discover_all};
pub use client::{RemoteLink, RemoteLinkOption, RemoteStateChecker};
pub use error::{PeerVersion, RejectKind, RemoteLinkError};
#[cfg(feature = "discovery")]
pub use mdns::{
    DiscoveryError, SERVICE_TYPE, SIM_SERVICE_TYPE, ServerKind, TXT_CONTROL_PORT, TXT_SDK_VERSION,
    TXT_WIRE_VERSION, instance_name,
};
pub use server::{
    Actual, BusOption, BusPacing, BusServer, BusServerOption, BusSnapshot, Desired, RemoteServer,
    RemoteServerOption, Session, Sessions, SharedBus,
};
#[cfg(feature = "discovery")]
pub use server::{Advertisement, AdvertisementHandle, advertise};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransducerLayout {
    pub pos: [f32; 3],
    pub dir: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceLayout {
    pub transducers: Vec<TransducerLayout>,
}
