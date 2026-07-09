use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

use crate::error::Error;
use crate::response::Response;

struct Inner {
    result: Option<Result<Response, Error>>,
    waker: Option<Waker>,
    sender_gone: bool,
}

pub(super) struct Completion {
    // `None` for the heap fallback handed out when the pool is empty.
    index: Option<usize>,
    refs: AtomicUsize,
    inner: Mutex<Inner>,
}

impl Completion {
    fn new(index: Option<usize>) -> Self {
        Self {
            index,
            refs: AtomicUsize::new(0),
            inner: Mutex::new(Inner {
                result: None,
                waker: None,
                sender_gone: false,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn complete(&self, result: Result<Response, Error>) {
        let waker = {
            let mut inner = self.lock();
            inner.result = Some(result);
            inner.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    fn close(&self) {
        let waker = {
            let mut inner = self.lock();
            inner.sender_gone = true;
            inner.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    fn poll(&self, cx: &Context<'_>) -> Poll<Result<Response, Error>> {
        let mut inner = self.lock();
        if let Some(result) = inner.result.take() {
            return Poll::Ready(result);
        }
        if inner.sender_gone {
            return Poll::Ready(Err(Error::RtClosed));
        }
        inner.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

pub(super) struct CompletionPool {
    entries: Vec<Arc<Completion>>,
    free: Mutex<Vec<usize>>,
}

impl CompletionPool {
    pub(super) fn new(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            entries: (0..capacity)
                .map(|index| Arc::new(Completion::new(Some(index))))
                .collect(),
            free: Mutex::new((0..capacity).rev().collect()),
        })
    }

    pub(super) fn channel(self: &Arc<Self>) -> (CompletionSender, ResponseFuture) {
        let index = self
            .free
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop();
        let completion = match index {
            Some(index) => Arc::clone(&self.entries[index]),
            None => Arc::new(Completion::new(None)),
        };
        completion.refs.store(2, Ordering::Relaxed);
        (
            CompletionSender {
                completion: Arc::clone(&completion),
                pool: Arc::clone(self),
                completed: false,
            },
            ResponseFuture {
                completion: Some(completion),
                pool: Arc::clone(self),
            },
        )
    }

    // Recycled by whichever of the two halves is dropped last.
    fn release(&self, completion: &Arc<Completion>) {
        if completion.refs.fetch_sub(1, Ordering::AcqRel) != 1 {
            return;
        }
        let Some(index) = completion.index else {
            return;
        };
        {
            let mut inner = completion.lock();
            inner.result = None;
            inner.waker = None;
            inner.sender_gone = false;
        }
        self.free
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(index);
    }
}

pub(super) struct CompletionSender {
    completion: Arc<Completion>,
    pool: Arc<CompletionPool>,
    completed: bool,
}

impl CompletionSender {
    pub(super) fn send(mut self, result: Result<Response, Error>) {
        self.completion.complete(result);
        self.completed = true;
    }
}

impl Drop for CompletionSender {
    fn drop(&mut self) {
        if !self.completed {
            self.completion.close();
        }
        self.pool.release(&self.completion);
    }
}

pub struct ResponseFuture {
    completion: Option<Arc<Completion>>,
    pool: Arc<CompletionPool>,
}

impl Future for ResponseFuture {
    type Output = Result<Response, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Some(completion) = self.completion.as_ref() else {
            return Poll::Ready(Err(Error::RtClosed));
        };
        match completion.poll(cx) {
            Poll::Ready(result) => {
                let completion = self.completion.take().expect("just checked");
                self.pool.release(&completion);
                Poll::Ready(result)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for ResponseFuture {
    fn drop(&mut self) {
        if let Some(completion) = self.completion.take() {
            self.pool.release(&completion);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::CompletionPool;
    use crate::error::Error;
    use crate::response::Response;

    fn free_len(pool: &CompletionPool) -> usize {
        pool.free.lock().unwrap().len()
    }

    #[tokio::test]
    async fn a_completed_channel_returns_to_the_pool() {
        let pool = CompletionPool::new(1);
        let (tx, rx) = pool.channel();
        assert_eq!(free_len(&pool), 0);

        tx.send(Ok(Response::from_slice(&[0x42])));
        assert_eq!(rx.await.unwrap().data(), [0x42]);
        assert_eq!(free_len(&pool), 1);
    }

    #[tokio::test]
    async fn dropping_the_sender_reports_a_closed_rt() {
        let pool = CompletionPool::new(1);
        let (tx, rx) = pool.channel();
        drop(tx);
        assert!(matches!(rx.await, Err(Error::RtClosed)));
        assert_eq!(free_len(&pool), 1);
    }

    #[tokio::test]
    async fn dropping_the_future_first_still_recycles() {
        let pool = CompletionPool::new(1);
        let (tx, rx) = pool.channel();
        drop(rx);
        assert_eq!(free_len(&pool), 0);
        tx.send(Ok(Response::from_slice(&[0])));
        assert_eq!(free_len(&pool), 1);
    }

    #[tokio::test]
    async fn an_exhausted_pool_falls_back_to_the_heap_instead_of_blocking() {
        let pool = CompletionPool::new(1);
        let (tx1, rx1) = pool.channel();
        let (tx2, rx2) = pool.channel();
        assert_eq!(free_len(&pool), 0);

        tx2.send(Ok(Response::from_slice(&[2])));
        assert_eq!(rx2.await.unwrap().data(), [2]);
        // The fallback is not pooled, so nothing is handed back for it.
        assert_eq!(free_len(&pool), 0);

        tx1.send(Ok(Response::from_slice(&[1])));
        assert_eq!(rx1.await.unwrap().data(), [1]);
        assert_eq!(free_len(&pool), 1);
    }

    #[tokio::test]
    async fn a_pending_future_is_woken_by_the_sender() {
        let pool = CompletionPool::new(1);
        let (tx, rx) = pool.channel();
        let joined = tokio::spawn(rx);
        tokio::task::yield_now().await;
        tx.send(Ok(Response::from_slice(&[7])));
        assert_eq!(joined.await.unwrap().unwrap().data(), [7]);
    }

    #[test]
    fn a_recycled_entry_starts_clean() {
        let pool = CompletionPool::new(1);
        let (tx, rx) = pool.channel();
        tx.send(Err(Error::RtClosed));
        drop(rx);
        let entry = &pool.entries[0];
        assert_eq!(entry.refs.load(Ordering::Relaxed), 0);
        let inner = entry.lock();
        assert!(inner.result.is_none());
        assert!(!inner.sender_gone);
    }
}
