use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_channel::oneshot;

use crate::error::Error;
use crate::response::Response;

pub(crate) struct CompletionSender {
    sender: oneshot::Sender<Result<Response, Error>>,
}

impl CompletionSender {
    pub(crate) fn send(self, result: Result<Response, Error>) {
        if let Ok(response) = &result
            && let Err(e) = response.check()
        {
            tracing::warn!(error = %e, "device reported an error");
        }
        let _ = self.sender.send(result);
    }
}

pub(crate) fn channel() -> (CompletionSender, ResponseFuture) {
    let (sender, receiver) = oneshot::channel();
    (CompletionSender { sender }, ResponseFuture { receiver })
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

#[derive(Default)]
pub struct StreamFuture {
    pending: VecDeque<ResponseFuture>,
    failed: Option<Error>,
    finished: bool,
}

impl StreamFuture {
    pub(crate) fn in_flight(&self) -> usize {
        self.pending.len()
    }

    pub(crate) fn has_failed(&self) -> bool {
        self.failed.is_some()
    }

    pub(crate) fn push(&mut self, response: ResponseFuture) {
        self.pending.push_back(response);
    }

    pub(crate) fn fail(&mut self, e: Error) {
        self.failed.get_or_insert(e);
    }

    pub(crate) async fn settle_oldest(&mut self) {
        if let Some(oldest) = self.pending.pop_front()
            && let Err(e) = oldest.await.and_then(|response| response.check())
        {
            self.fail(e);
        }
    }
}

impl Future for StreamFuture {
    type Output = Result<(), Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        if this.finished {
            return Poll::Ready(Err(Error::Closed));
        }
        while let Some(front) = this.pending.front_mut() {
            let Poll::Ready(result) = Pin::new(front).poll(cx) else {
                return Poll::Pending;
            };
            this.pending.pop_front();
            if let Err(e) = result.and_then(|response| response.check()) {
                this.fail(e);
            }
        }
        this.finished = true;
        Poll::Ready(this.failed.take().map_or(Ok(()), Err))
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
        tx.send(Ok(Response::from_status(&[0x42])));
        assert_eq!(rx.await.unwrap().status(), [0x42]);
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
        tx.send(Ok(Response::from_status(&[7])));
        assert_eq!(joined.await.unwrap().unwrap().status(), [7]);
    }

    #[tokio::test]
    async fn polling_after_completion_reports_a_closed_driver() {
        let (tx, mut rx) = channel();
        tx.send(Ok(Response::from_status(&[1])));
        assert_eq!((&mut rx).await.unwrap().status(), [1]);
        assert!(matches!(rx.await, Err(Error::Closed)));
    }
}
