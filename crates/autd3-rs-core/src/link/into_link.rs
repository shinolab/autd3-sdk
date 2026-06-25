use super::Link;
use crate::geometry::Geometry;

pub trait IntoLink: Send {
    type Link: Link;

    fn into_link(
        self,
        geometry: &Geometry,
    ) -> impl Future<Output = Result<Self::Link, crate::Error>> + Send;
}

impl<L: Link> IntoLink for L {
    type Link = L;

    async fn into_link(self, _geometry: &Geometry) -> Result<L, crate::Error> {
        Ok(self)
    }
}
