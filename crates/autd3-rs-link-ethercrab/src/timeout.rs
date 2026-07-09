use std::future::Future;
use std::time::Duration;

use ethercrab::Timeouts;
use ethercrab::error::{Error, TimeoutError};

// ethercrab arms an `async_io::Timer` for every PDU, and registering one costs a
// `BTreeMap` node in the async-io reactor on every bus cycle. `Timer::after` falls
// back to a timer that is never registered when the deadline overflows, so an
// infinite `pdu` timeout keeps the hot path allocation-free. Every call site that
// relied on it must apply `with_timeout` instead.
pub(crate) fn without_pdu_timer(timeouts: Timeouts) -> Timeouts {
    Timeouts {
        pdu: Duration::MAX,
        ..timeouts
    }
}

pub(crate) async fn with_timeout<T, F>(
    timeout: Duration,
    kind: TimeoutError,
    future: F,
) -> Result<T, Error>
where
    F: Future<Output = Result<T, Error>>,
{
    tokio::time::timeout(timeout, future)
        .await
        .unwrap_or_else(|_| Err(Error::Timeout(kind)))
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Waker};
    use std::time::{Duration, Instant};

    use ethercrab::Timeouts;
    use ethercrab::error::{Error, TimeoutError};

    use super::{with_timeout, without_pdu_timer};

    #[test]
    fn an_overflowing_deadline_is_what_disarms_the_ethercrab_pdu_timer() {
        assert!(Instant::now().checked_add(Duration::MAX).is_none());
        assert_eq!(without_pdu_timer(Timeouts::default()).pdu, Duration::MAX);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn an_async_io_timer_with_an_infinite_duration_stays_pending() {
        let mut timer = async_io::Timer::after(Duration::MAX);
        let mut cx = Context::from_waker(Waker::noop());
        assert!(Pin::new(&mut timer).poll(&mut cx).is_pending());
    }

    #[tokio::test]
    async fn an_elapsed_timeout_becomes_an_ethercrab_timeout_error() {
        let never = std::future::pending::<Result<(), Error>>();
        let result = with_timeout(Duration::from_millis(1), TimeoutError::Pdu, never).await;
        assert!(matches!(result, Err(Error::Timeout(TimeoutError::Pdu))));
    }

    #[tokio::test]
    async fn a_completed_future_passes_its_value_through() {
        let ready = std::future::ready(Ok::<u8, Error>(7));
        let result = with_timeout(Duration::from_secs(1), TimeoutError::Pdu, ready).await;
        assert_eq!(result.unwrap(), 7);
    }
}
