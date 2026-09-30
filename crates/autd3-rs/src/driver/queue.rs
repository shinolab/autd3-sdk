use std::collections::VecDeque;
use std::num::{NonZeroU32, NonZeroUsize};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use autd3_rs_core::rt::oneshot;

use crate::client::completion::CompletionSender;
use crate::client::pool::Slot;
use crate::error::NetworkCause;

pub(crate) struct CmdMessage {
    pub(crate) frame: Slot,
    pub(crate) response_tx: CompletionSender,
    pub(crate) exclusive: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TransportConfig {
    pub(crate) ack_timeout: Duration,
    pub(crate) max_inflight: NonZeroUsize,
    pub(crate) max_resync_rounds: NonZeroU32,
    pub(crate) low_latency: bool,
}

pub(crate) struct Connect {
    pub(crate) config: TransportConfig,
    pub(crate) done: oneshot::Sender<Result<(), NetworkCause>>,
}

pub(crate) struct Requests {
    pub(crate) connect: Option<Connect>,
    pub(crate) close: bool,
    pub(crate) generation: u64,
}

#[derive(Default)]
struct State {
    cmds: VecDeque<CmdMessage>,
    connect: Option<Connect>,
    close: bool,
    closed: bool,
    generation: u64,
    next_key: u64,
    wakers: Vec<(u64, Waker)>,
}

impl State {
    fn bump(&mut self) -> Vec<(u64, Waker)> {
        self.generation = self.generation.wrapping_add(1);
        std::mem::take(&mut self.wakers)
    }
}

#[derive(Default)]
pub(crate) struct Queue {
    state: Mutex<State>,
}

fn wake_all(wakers: Vec<(u64, Waker)>) {
    for (_, waker) in wakers {
        waker.wake();
    }
}

impl Queue {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn push_with(&self, f: impl FnOnce(&mut State)) -> bool {
        let wakers = {
            let mut state = self.lock();
            if state.closed {
                return false;
            }
            f(&mut state);
            state.bump()
        };
        wake_all(wakers);
        true
    }

    pub(crate) fn push_cmd(&self, msg: CmdMessage) -> Result<(), CmdMessage> {
        let mut msg = Some(msg);
        if self.push_with(|state| state.cmds.extend(msg.take())) {
            Ok(())
        } else {
            Err(msg.expect("not pushed"))
        }
    }

    pub(crate) fn push_connect(&self, connect: Connect) -> Result<(), Connect> {
        let mut connect = Some(connect);
        if self.push_with(|state| state.connect = connect.take()) {
            Ok(())
        } else {
            Err(connect.expect("not pushed"))
        }
    }

    pub(crate) fn request_close(&self) {
        self.push_with(|state| state.close = true);
    }

    pub(crate) fn requests(&self) -> Requests {
        let mut state = self.lock();
        Requests {
            connect: state.connect.take(),
            close: state.close,
            generation: state.generation,
        }
    }

    pub(crate) fn pop_cmd(&self) -> Option<CmdMessage> {
        self.lock().cmds.pop_front()
    }

    pub(crate) fn shut(&self) -> (Vec<CmdMessage>, Option<Connect>) {
        let (cmds, connect, wakers) = {
            let mut state = self.lock();
            state.closed = true;
            let cmds = state.cmds.drain(..).collect();
            let connect = state.connect.take();
            (cmds, connect, state.bump())
        };
        wake_all(wakers);
        (cmds, connect)
    }

    pub(crate) fn changed_since(&self, seen: u64) -> bool {
        self.lock().generation != seen
    }

    pub(crate) fn register(&self, seen: u64, waker: &Waker, key: &mut Option<u64>) -> bool {
        let mut state = self.lock();
        if state.generation != seen {
            return true;
        }
        if let Some(k) = *key
            && let Some((_, registered)) = state.wakers.iter_mut().find(|(id, _)| *id == k)
        {
            if !registered.will_wake(waker) {
                registered.clone_from(waker);
            }
            return false;
        }
        let k = state.next_key;
        state.next_key = state.next_key.wrapping_add(1);
        state.wakers.push((k, waker.clone()));
        *key = Some(k);
        false
    }

    fn deregister(&self, key: u64) {
        self.lock().wakers.retain(|(id, _)| *id != key);
    }
}

pub(crate) struct Link {
    pub(crate) queue: Arc<Queue>,
}

impl Drop for Link {
    fn drop(&mut self) {
        self.queue.request_close();
    }
}

#[must_use = "futures do nothing unless polled"]
pub struct Notified {
    queue: Arc<Queue>,
    seen: u64,
    key: Option<u64>,
}

impl Notified {
    pub(crate) fn new(queue: Arc<Queue>, seen: u64) -> Self {
        Self {
            queue,
            seen,
            key: None,
        }
    }
}

impl Drop for Notified {
    fn drop(&mut self) {
        if let Some(key) = self.key.take() {
            self.queue.deregister(key);
        }
    }
}

impl Future for Notified {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        if this.queue.register(this.seen, cx.waker(), &mut this.key) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

impl core::fmt::Debug for Notified {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Notified").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registered(queue: &Queue) -> usize {
        queue.lock().wakers.len()
    }

    #[test]
    fn a_dropped_waiter_leaves_no_waker_behind() {
        let queue = Queue::new();
        let mut cx = Context::from_waker(Waker::noop());
        for _ in 0..100 {
            let mut notified = Notified::new(Arc::clone(&queue), 0);
            assert!(Pin::new(&mut notified).poll(&mut cx).is_pending());
            assert!(Pin::new(&mut notified).poll(&mut cx).is_pending());
            assert_eq!(registered(&queue), 1);
        }
        assert_eq!(registered(&queue), 0);
    }

    #[test]
    fn a_waiter_completes_on_a_request_made_after_the_poll() {
        let queue = Queue::new();
        let seen = queue.requests().generation;
        let mut notified = Notified::new(Arc::clone(&queue), seen);
        let mut cx = Context::from_waker(Waker::noop());
        assert!(Pin::new(&mut notified).poll(&mut cx).is_pending());
        queue.request_close();
        assert!(Pin::new(&mut notified).poll(&mut cx).is_ready());
        assert_eq!(registered(&queue), 0);
    }
}
