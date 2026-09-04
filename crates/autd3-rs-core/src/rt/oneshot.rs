use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the sender was dropped without sending a value")]
pub struct Canceled;

struct Inner<T> {
    value: Option<T>,
    waker: Option<Waker>,
    sender_gone: bool,
    receiver_gone: bool,
}

struct Shared<T> {
    inner: Mutex<Inner<T>>,
}

impl<T> Shared<T> {
    fn lock(&self) -> MutexGuard<'_, Inner<T>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub struct Sender<T> {
    shared: Arc<Shared<T>>,
    sent: bool,
}

pub struct Receiver<T> {
    shared: Arc<Shared<T>>,
    done: bool,
}

#[must_use]
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared {
        inner: Mutex::new(Inner {
            value: None,
            waker: None,
            sender_gone: false,
            receiver_gone: false,
        }),
    });
    (
        Sender {
            shared: Arc::clone(&shared),
            sent: false,
        },
        Receiver {
            shared,
            done: false,
        },
    )
}

impl<T> Sender<T> {
    pub fn send(mut self, value: T) -> Result<(), T> {
        self.sent = true;
        let waker = {
            let mut inner = self.shared.lock();
            if inner.receiver_gone {
                return Err(value);
            }
            inner.value = Some(value);
            inner.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
        Ok(())
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.shared.lock().receiver_gone
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if self.sent {
            return;
        }
        let waker = {
            let mut inner = self.shared.lock();
            inner.sender_gone = true;
            inner.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl<T> Receiver<T> {
    pub fn try_recv(&mut self) -> Option<Result<T, Canceled>> {
        if self.done {
            return Some(Err(Canceled));
        }
        let result = {
            let mut inner = self.shared.lock();
            match inner.value.take() {
                Some(value) => Some(Ok(value)),
                None if inner.sender_gone => Some(Err(Canceled)),
                None => None,
            }
        };
        if result.is_some() {
            self.done = true;
        }
        result
    }
}

impl<T> core::fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Sender").finish_non_exhaustive()
    }
}

impl<T> core::fmt::Debug for Receiver<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Receiver").finish_non_exhaustive()
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.shared.lock().receiver_gone = true;
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, Canceled>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(Err(Canceled));
        }
        let result = {
            let mut inner = this.shared.lock();
            match inner.value.take() {
                Some(value) => Some(Ok(value)),
                None if inner.sender_gone => Some(Err(Canceled)),
                None => {
                    inner.waker = Some(cx.waker().clone());
                    None
                }
            }
        };
        match result {
            Some(result) => {
                this.done = true;
                Poll::Ready(result)
            }
            None => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Poll, Wake, Waker};

    use super::{Canceled, channel};

    struct CountingWaker(AtomicUsize);

    impl Wake for CountingWaker {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn counting_waker() -> (Arc<CountingWaker>, Waker) {
        let inner = Arc::new(CountingWaker(AtomicUsize::new(0)));
        let waker = Waker::from(Arc::clone(&inner));
        (inner, waker)
    }

    #[test]
    fn send_before_poll_is_ready() {
        let (tx, rx) = channel::<u32>();
        assert_eq!(tx.send(7), Ok(()));
        let (_, waker) = counting_waker();
        let mut rx = Box::pin(rx);
        assert_eq!(
            rx.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Ready(Ok(7))
        );
    }

    #[test]
    fn pending_until_sent_then_wakes_once() {
        let (tx, rx) = channel::<u32>();
        let (count, waker) = counting_waker();
        let mut rx = Box::pin(rx);
        assert_eq!(
            rx.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Pending
        );
        assert_eq!(count.0.load(Ordering::Relaxed), 0);
        assert_eq!(tx.send(3), Ok(()));
        assert_eq!(count.0.load(Ordering::Relaxed), 1);
        assert_eq!(
            rx.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Ready(Ok(3))
        );
    }

    #[test]
    fn dropped_sender_cancels() {
        let (tx, rx) = channel::<u32>();
        let (count, waker) = counting_waker();
        let mut rx = Box::pin(rx);
        assert_eq!(
            rx.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Pending
        );
        drop(tx);
        assert_eq!(count.0.load(Ordering::Relaxed), 1);
        assert_eq!(
            rx.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Ready(Err(Canceled))
        );
    }

    #[test]
    fn value_survives_a_dropped_sender() {
        let (tx, rx) = channel::<u32>();
        assert_eq!(tx.send(11), Ok(()));
        let (_, waker) = counting_waker();
        let mut rx = Box::pin(rx);
        assert_eq!(
            rx.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Ready(Ok(11))
        );
    }

    #[test]
    fn polling_after_completion_reports_canceled() {
        let (tx, rx) = channel::<u32>();
        assert_eq!(tx.send(1), Ok(()));
        let (_, waker) = counting_waker();
        let mut rx = Box::pin(rx);
        assert_eq!(
            rx.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Ready(Ok(1))
        );
        assert_eq!(
            rx.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Ready(Err(Canceled))
        );
    }

    #[test]
    fn try_recv_does_not_block() {
        let (tx, mut rx) = channel::<u32>();
        assert!(rx.try_recv().is_none());
        assert_eq!(tx.send(5), Ok(()));
        assert_eq!(rx.try_recv(), Some(Ok(5)));
        assert_eq!(rx.try_recv(), Some(Err(Canceled)));
    }

    #[test]
    fn try_recv_reports_a_dropped_sender() {
        let (tx, mut rx) = channel::<u32>();
        drop(tx);
        assert_eq!(rx.try_recv(), Some(Err(Canceled)));
    }

    #[test]
    fn a_racing_sender_never_loses_a_wakeup() {
        for round in 0..2000u32 {
            let (tx, rx) = channel::<u32>();
            let sender = std::thread::spawn(move || {
                let _ = tx.send(round);
            });
            assert_eq!(crate::rt::block_on(rx), Ok(round));
            sender.join().unwrap();
        }
    }

    #[test]
    fn a_racing_dropped_sender_never_loses_a_wakeup() {
        for _ in 0..2000u32 {
            let (tx, rx) = channel::<u32>();
            let sender = std::thread::spawn(move || drop(tx));
            assert_eq!(crate::rt::block_on(rx), Err(Canceled));
            sender.join().unwrap();
        }
    }

    #[test]
    fn a_dropped_receiver_hands_the_value_back() {
        let (tx, rx) = channel::<u32>();
        assert!(!tx.is_closed());
        drop(rx);
        assert!(tx.is_closed());
        assert_eq!(tx.send(9), Err(9));
    }
}
