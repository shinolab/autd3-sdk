use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

struct WaiterState {
    waker: Option<Waker>,
    granted: bool,
}

type Waiter = Arc<Mutex<WaiterState>>;

fn lock_waiter(waiter: &Waiter) -> MutexGuard<'_, WaiterState> {
    waiter.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Inner {
    permits: usize,
    waiters: VecDeque<Waiter>,
}

impl Inner {
    fn grant_next(&mut self, waker: &mut Option<Waker>) -> bool {
        let Some(waiter) = self.waiters.pop_front() else {
            return false;
        };
        let mut state = lock_waiter(&waiter);
        state.granted = true;
        *waker = state.waker.take();
        true
    }
}

pub struct Semaphore {
    inner: Mutex<Inner>,
}

impl Semaphore {
    #[must_use]
    pub fn new(permits: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                permits,
                waiters: VecDeque::new(),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[must_use]
    pub fn available_permits(&self) -> usize {
        self.lock().permits
    }

    pub fn add_permits(&self, n: usize) {
        for _ in 0..n {
            self.release_one();
        }
    }

    fn release_one(&self) {
        let mut waker = None;
        {
            let mut inner = self.lock();
            if !inner.grant_next(&mut waker) {
                inner.permits += 1;
            }
        }
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    #[must_use]
    pub fn acquire(&self) -> Acquire<'_> {
        Acquire {
            semaphore: self,
            waiter: None,
            done: false,
        }
    }

    #[must_use]
    pub fn try_acquire(&self) -> Option<SemaphorePermit<'_>> {
        let mut inner = self.lock();
        if inner.permits == 0 {
            return None;
        }
        inner.permits -= 1;
        drop(inner);
        Some(SemaphorePermit {
            semaphore: Some(self),
        })
    }
}

impl core::fmt::Debug for Semaphore {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Semaphore")
            .field("permits", &self.available_permits())
            .finish_non_exhaustive()
    }
}

pub struct Acquire<'a> {
    semaphore: &'a Semaphore,
    waiter: Option<Waiter>,
    done: bool,
}

impl<'a> Future for Acquire<'a> {
    type Output = SemaphorePermit<'a>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        assert!(!this.done, "acquire polled after completion");
        let semaphore = this.semaphore;
        let ready = {
            let mut inner = semaphore.lock();
            if let Some(waiter) = this.waiter.as_ref() {
                let mut state = lock_waiter(waiter);
                if state.granted {
                    true
                } else {
                    state.waker = Some(cx.waker().clone());
                    false
                }
            } else if inner.permits > 0 {
                inner.permits -= 1;
                true
            } else {
                let waiter = Arc::new(Mutex::new(WaiterState {
                    waker: Some(cx.waker().clone()),
                    granted: false,
                }));
                inner.waiters.push_back(Arc::clone(&waiter));
                this.waiter = Some(waiter);
                false
            }
        };
        if ready {
            this.waiter = None;
            this.done = true;
            Poll::Ready(SemaphorePermit {
                semaphore: Some(semaphore),
            })
        } else {
            Poll::Pending
        }
    }
}

impl Drop for Acquire<'_> {
    fn drop(&mut self) {
        let Some(waiter) = self.waiter.take() else {
            return;
        };
        let granted = {
            let mut inner = self.semaphore.lock();
            let granted = lock_waiter(&waiter).granted;
            if !granted {
                inner.waiters.retain(|queued| !Arc::ptr_eq(queued, &waiter));
            }
            granted
        };
        if granted {
            self.semaphore.release_one();
        }
    }
}

pub struct SemaphorePermit<'a> {
    semaphore: Option<&'a Semaphore>,
}

impl SemaphorePermit<'_> {
    pub fn forget(mut self) {
        self.semaphore = None;
    }
}

impl Drop for SemaphorePermit<'_> {
    fn drop(&mut self) {
        if let Some(semaphore) = self.semaphore {
            semaphore.release_one();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Poll, Wake, Waker};

    use super::Semaphore;

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
    fn acquire_takes_a_permit_immediately() {
        let semaphore = Semaphore::new(2);
        let (_, waker) = counting_waker();
        let mut acquire = Box::pin(semaphore.acquire());
        let Poll::Ready(permit) = acquire.as_mut().poll(&mut Context::from_waker(&waker)) else {
            panic!("a permit is available");
        };
        assert_eq!(semaphore.available_permits(), 1);
        drop(permit);
        assert_eq!(semaphore.available_permits(), 2);
    }

    #[test]
    fn permit_is_returned_on_drop() {
        let semaphore = Semaphore::new(1);
        let permit = semaphore.try_acquire().expect("a permit is available");
        assert_eq!(semaphore.available_permits(), 0);
        drop(permit);
        assert_eq!(semaphore.available_permits(), 1);
    }

    #[test]
    fn forget_keeps_the_permit_consumed() {
        let semaphore = Semaphore::new(1);
        semaphore
            .try_acquire()
            .expect("a permit is available")
            .forget();
        assert_eq!(semaphore.available_permits(), 0);
        semaphore.add_permits(1);
        assert_eq!(semaphore.available_permits(), 1);
    }

    #[test]
    fn waiter_is_woken_when_a_permit_is_added() {
        let semaphore = Semaphore::new(0);
        let (count, waker) = counting_waker();
        let mut acquire = Box::pin(semaphore.acquire());
        assert!(
            acquire
                .as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
        assert_eq!(count.0.load(Ordering::Relaxed), 0);
        semaphore.add_permits(1);
        assert_eq!(count.0.load(Ordering::Relaxed), 1);
        assert_eq!(semaphore.available_permits(), 0);
        assert!(
            acquire
                .as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_ready()
        );
    }

    #[test]
    fn waiters_are_served_in_order() {
        let semaphore = Semaphore::new(0);
        let (first_count, first_waker) = counting_waker();
        let (second_count, second_waker) = counting_waker();
        let mut first = Box::pin(semaphore.acquire());
        let mut second = Box::pin(semaphore.acquire());
        assert!(
            first
                .as_mut()
                .poll(&mut Context::from_waker(&first_waker))
                .is_pending()
        );
        assert!(
            second
                .as_mut()
                .poll(&mut Context::from_waker(&second_waker))
                .is_pending()
        );
        semaphore.add_permits(1);
        assert_eq!(first_count.0.load(Ordering::Relaxed), 1);
        assert_eq!(second_count.0.load(Ordering::Relaxed), 0);
        assert!(
            second
                .as_mut()
                .poll(&mut Context::from_waker(&second_waker))
                .is_pending()
        );
        assert!(
            first
                .as_mut()
                .poll(&mut Context::from_waker(&first_waker))
                .is_ready()
        );
    }

    #[test]
    fn cancelling_a_waiter_does_not_lose_the_permit() {
        let semaphore = Semaphore::new(0);
        let (_, first_waker) = counting_waker();
        let (second_count, second_waker) = counting_waker();
        let mut first = Box::pin(semaphore.acquire());
        let mut second = Box::pin(semaphore.acquire());
        assert!(
            first
                .as_mut()
                .poll(&mut Context::from_waker(&first_waker))
                .is_pending()
        );
        assert!(
            second
                .as_mut()
                .poll(&mut Context::from_waker(&second_waker))
                .is_pending()
        );
        drop(first);
        semaphore.add_permits(1);
        assert_eq!(second_count.0.load(Ordering::Relaxed), 1);
        assert!(
            second
                .as_mut()
                .poll(&mut Context::from_waker(&second_waker))
                .is_ready()
        );
    }

    #[test]
    fn dropping_a_granted_waiter_returns_the_permit() {
        let semaphore = Semaphore::new(0);
        let (_, waker) = counting_waker();
        let mut acquire = Box::pin(semaphore.acquire());
        assert!(
            acquire
                .as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
        semaphore.add_permits(1);
        assert_eq!(semaphore.available_permits(), 0);
        drop(acquire);
        assert_eq!(semaphore.available_permits(), 1);
    }

    #[test]
    fn contended_acquires_conserve_every_permit() {
        const THREADS: usize = 8;
        const ROUNDS: usize = 200;
        const PERMITS: usize = 3;

        let semaphore = Arc::new(Semaphore::new(PERMITS));
        let live = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let threads = (0..THREADS)
            .map(|_| {
                let semaphore = Arc::clone(&semaphore);
                let live = Arc::clone(&live);
                let peak = Arc::clone(&peak);
                std::thread::spawn(move || {
                    for _ in 0..ROUNDS {
                        let permit = crate::rt::block_on(semaphore.acquire());
                        let held = live.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(held, Ordering::SeqCst);
                        std::thread::yield_now();
                        live.fetch_sub(1, Ordering::SeqCst);
                        drop(permit);
                    }
                })
            })
            .collect::<Vec<_>>();
        for thread in threads {
            thread.join().unwrap();
        }
        assert!(peak.load(Ordering::SeqCst) <= PERMITS);
        assert_eq!(semaphore.available_permits(), PERMITS);
    }

    #[test]
    fn cancelled_acquires_under_contention_conserve_every_permit() {
        const THREADS: usize = 8;
        const ROUNDS: usize = 200;
        const PERMITS: usize = 2;

        let semaphore = Arc::new(Semaphore::new(PERMITS));
        let threads = (0..THREADS)
            .map(|index| {
                let semaphore = Arc::clone(&semaphore);
                std::thread::spawn(move || {
                    for round in 0..ROUNDS {
                        if (round + index) % 2 == 0 {
                            drop(crate::rt::block_on(semaphore.acquire()));
                        } else {
                            let (_, waker) = counting_waker();
                            let mut acquire = Box::pin(semaphore.acquire());
                            let _ = acquire.as_mut().poll(&mut Context::from_waker(&waker));
                            drop(acquire);
                        }
                    }
                })
            })
            .collect::<Vec<_>>();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(semaphore.available_permits(), PERMITS);
    }

    #[test]
    fn a_cancelled_waiter_leaves_the_queue_immediately() {
        let semaphore = Semaphore::new(0);
        let (_, waker) = counting_waker();
        for _ in 0..1000 {
            let mut acquire = Box::pin(semaphore.acquire());
            assert!(
                acquire
                    .as_mut()
                    .poll(&mut Context::from_waker(&waker))
                    .is_pending()
            );
        }
        assert!(semaphore.lock().waiters.is_empty());
    }

    #[test]
    fn cancelling_every_waiter_restores_the_permit_count() {
        let semaphore = Semaphore::new(0);
        let (_, waker) = counting_waker();
        let mut acquire = Box::pin(semaphore.acquire());
        assert!(
            acquire
                .as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
        drop(acquire);
        semaphore.add_permits(1);
        assert_eq!(semaphore.available_permits(), 1);
    }
}
