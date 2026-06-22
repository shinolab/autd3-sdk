use tokio::sync::oneshot;

use crate::error::Error;
use crate::response::Response;

pub struct ResponseFuture {
    pub(super) rx: oneshot::Receiver<Result<Response, Error>>,
}

impl std::future::Future for ResponseFuture {
    type Output = Result<Response, Error>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match std::pin::Pin::new(&mut self.rx).poll(cx) {
            std::task::Poll::Ready(Ok(inner)) => std::task::Poll::Ready(inner),
            std::task::Poll::Ready(Err(_)) => std::task::Poll::Ready(Err(Error::RtClosed)),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}
