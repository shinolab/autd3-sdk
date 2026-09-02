use super::Link;
use crate::geometry::Geometry;

pub trait IntoLink: Send {
    type Link: Link;

    fn into_link(self, geometry: &Geometry) -> Result<Self::Link, crate::error::LinkError>;
}

impl<L: Link> IntoLink for L {
    type Link = L;

    fn into_link(self, _geometry: &Geometry) -> Result<L, crate::error::LinkError> {
        Ok(self)
    }
}
