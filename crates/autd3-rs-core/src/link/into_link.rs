use super::Link;

pub trait IntoLink: Send {
    type Link: Link;

    fn into_link(self) -> impl Future<Output = Result<Self::Link, crate::Error>> + Send;
}

impl<L: Link> IntoLink for L {
    type Link = L;

    async fn into_link(self) -> Result<L, crate::Error> {
        Ok(self)
    }
}
