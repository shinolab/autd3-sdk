use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::task::Poll;

pub(crate) async fn join_bounded<const N: usize, F: Future>(
    futures: impl ExactSizeIterator<Item = F>,
) -> [Option<F::Output>; N] {
    assert!(
        futures.len() <= N,
        "join_bounded: {} futures exceed the capacity of {N}",
        futures.len()
    );

    let mut slots: [Option<F>; N] = [const { None }; N];
    for (slot, future) in slots.iter_mut().zip(futures) {
        *slot = Some(future);
    }

    let mut outputs: [Option<F::Output>; N] = [const { None }; N];
    poll_fn(|cx| {
        let mut pending = false;
        for (slot, output) in slots.iter_mut().zip(outputs.iter_mut()) {
            let Some(future) = slot.as_mut() else {
                continue;
            };
            // SAFETY: `slots` is a local of this `async fn`, so it lives inside the
            // future returned by it. The caller cannot move that future between polls
            // once it has been pinned, and it is only polled through a `Pin`, so
            // `slots` never moves after this projection is first created. Completed
            // futures are dropped in place by assigning `None`, never moved out.
            match unsafe { Pin::new_unchecked(future) }.poll(cx) {
                Poll::Ready(value) => {
                    *output = Some(value);
                    *slot = None;
                }
                Poll::Pending => pending = true,
            }
        }
        if pending {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;

    outputs
}

#[cfg(test)]
mod tests {
    use std::future::ready;
    use std::pin::Pin;

    use super::join_bounded;

    #[tokio::test]
    async fn collects_outputs_in_order_and_leaves_unused_slots_empty() {
        let outputs = join_bounded::<4, _>([ready(1), ready(2), ready(3)].into_iter()).await;
        assert_eq!(outputs, [Some(1), Some(2), Some(3), None]);
    }

    #[tokio::test]
    async fn drives_futures_concurrently() {
        let (tx, rx) = tokio::sync::oneshot::channel::<u8>();
        // `waiter` is polled first and returns `Pending`; it can only complete if
        // `sender` is polled within the same call, i.e. the join does not serialise.
        let futures: [Pin<Box<dyn Future<Output = u8>>>; 2] = [
            Box::pin(async { rx.await.expect("sender alive") }),
            Box::pin(async {
                tx.send(7).expect("receiver alive");
                0
            }),
        ];
        let outputs = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            join_bounded::<2, _>(futures.into_iter()),
        )
        .await
        .expect("join completed");
        assert_eq!(outputs, [Some(7), Some(0)]);
    }

    #[tokio::test]
    #[should_panic(expected = "exceed the capacity")]
    async fn panics_when_the_capacity_is_too_small() {
        let _ = join_bounded::<1, _>([ready(1), ready(2)].into_iter()).await;
    }
}
