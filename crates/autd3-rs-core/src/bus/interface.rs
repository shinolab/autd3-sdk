use std::net::SocketAddrV6;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Interface {
    #[default]
    Auto,
    Name(String),
    Simulator,
    Addr(SocketAddrV6),
}

impl Interface {
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        match self {
            Interface::Name(name) => Some(name),
            Interface::Auto | Interface::Simulator | Interface::Addr(_) => None,
        }
    }
}

impl From<String> for Interface {
    fn from(name: String) -> Self {
        Interface::Name(name)
    }
}

impl From<&str> for Interface {
    fn from(name: &str) -> Self {
        Interface::Name(name.to_owned())
    }
}

impl<T: Into<Interface>> From<Option<T>> for Interface {
    fn from(opt: Option<T>) -> Self {
        opt.map_or(Interface::Auto, Into::into)
    }
}
