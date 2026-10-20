use std::pin::Pin;
use std::task::{Context, Poll};

use futures_channel::oneshot;

use crate::error::Error;
use crate::response::Response;

pub(crate) type ResponseSender = oneshot::Sender<Result<Response, Error>>;

pub(crate) fn channel() -> (ResponseSender, ResponseFuture) {
    let (sender, receiver) = oneshot::channel();
    (sender, ResponseFuture { receiver })
}

pub struct ResponseFuture {
    receiver: oneshot::Receiver<Result<Response, Error>>,
}

impl Future for ResponseFuture {
    type Output = Result<Response, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.receiver)
            .poll(cx)
            .map(|received| received.unwrap_or(Err(Error::Closed)))
    }
}

#[cfg(test)]
mod tests {
    use super::channel;
    use crate::error::Error;
    use crate::response::Response;

    #[tokio::test]
    async fn a_sent_result_reaches_the_future() {
        let (tx, rx) = channel();
        tx.send(Ok(Response::from_error_codes(&[0x42]))).unwrap();
        assert_eq!(rx.await.unwrap().error_codes(), [0x42]);
    }

    #[tokio::test]
    async fn dropping_the_sender_reports_a_closed_driver() {
        let (tx, rx) = channel();
        drop(tx);
        assert!(matches!(rx.await, Err(Error::Closed)));
    }

    #[tokio::test]
    async fn a_pending_future_is_woken_by_the_sender() {
        let (tx, rx) = channel();
        let joined = tokio::spawn(rx);
        tokio::task::yield_now().await;
        tx.send(Ok(Response::from_error_codes(&[7]))).unwrap();
        assert_eq!(joined.await.unwrap().unwrap().error_codes(), [7]);
    }

    #[tokio::test]
    async fn polling_after_completion_reports_a_closed_driver() {
        let (tx, mut rx) = channel();
        tx.send(Ok(Response::from_error_codes(&[1]))).unwrap();
        assert_eq!((&mut rx).await.unwrap().error_codes(), [1]);
        assert!(matches!(rx.await, Err(Error::Closed)));
    }
}
