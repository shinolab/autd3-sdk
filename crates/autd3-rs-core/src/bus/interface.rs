use std::net::SocketAddrV6;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Interface {
    #[default]
    Auto,
    Name(String),
    Addr(SocketAddrV6),
}

impl From<String> for Interface {
    fn from(name: String) -> Self {
        Interface::Name(name)
    }
}
