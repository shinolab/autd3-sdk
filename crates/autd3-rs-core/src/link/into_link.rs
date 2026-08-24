use super::Link;
use crate::geometry::Geometry;

pub trait IntoLink: Send {
    type Link: Link;

    fn into_link(
        self,
        geometry: &Geometry,
    ) -> impl Future<Output = Result<Self::Link, crate::error::LinkError>> + Send;
}

impl<L: Link> IntoLink for L {
    type Link = L;

    fn into_link(
        self,
        _geometry: &Geometry,
    ) -> impl Future<Output = Result<L, crate::error::LinkError>> + Send {
        std::future::ready(Ok(self))
    }
}
