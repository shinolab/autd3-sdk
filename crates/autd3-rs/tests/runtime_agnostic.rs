use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use autd3_rs::commands::SetSilencer;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::rt::{Executor, block_on, oneshot};
use autd3_rs::{Client, ClientConfig};
use autd3_rs_firmware_emulator::Audit;

fn geometry(n: usize) -> Geometry {
    Geometry::new((0..n).map(|_| Autd3::default()).collect())
}

fn audit(n: usize) -> Audit {
    Audit::new((0..n).map(|_| Autd3::NUM_TRANSDUCERS))
}

async fn stream_silencer(client: &Client, rounds: usize) -> Result<(), autd3_rs::Error> {
    for _ in 0..rounds {
        let frames = client
            .datagram_builder()
            .push(SetSilencer::default())
            .build()?;
        for frame in &frames {
            client.send_checked(frame).await?;
        }
    }
    Ok(())
}

#[test]
fn a_client_opens_and_closes_without_an_async_runtime() {
    block_on(async {
        let client = Client::open(&geometry(2), audit(2), ClientConfig::default())
            .await
            .unwrap();
        assert_eq!(client.num_devices(), 2);
        stream_silencer(&client, 4).await.unwrap();
        client.stop().await.unwrap();
        client.close().await.unwrap();
    });
}

#[test]
fn a_link_failure_surfaces_through_close_without_an_async_runtime() {
    block_on(async {
        let client = Client::open(&geometry(1), audit(1), ClientConfig::default())
            .await
            .unwrap();
        let versions = client.read_firmware_version().await.unwrap();
        assert_eq!(versions.len(), 1);
        client.close().await.unwrap();
        assert!(client.close().await.is_ok());
    });
}

#[test]
fn more_concurrent_sends_than_slots_all_complete_on_one_executor_thread() {
    const SENDERS: usize = 12;
    let max_inflight = NonZeroUsize::new(3).unwrap();

    let client = Arc::new(
        block_on(Client::open(
            &geometry(1),
            audit(1),
            ClientConfig {
                max_inflight,
                ..ClientConfig::default()
            },
        ))
        .unwrap(),
    );

    let executor = Executor::new();
    let completed = Arc::new(AtomicUsize::new(0));
    let (done_tx, done_rx) = oneshot::channel::<()>();
    let done_tx = Arc::new(std::sync::Mutex::new(Some(done_tx)));

    for _ in 0..SENDERS {
        let client = Arc::clone(&client);
        let completed = Arc::clone(&completed);
        let done_tx = Arc::clone(&done_tx);
        assert!(executor.spawn(async move {
            stream_silencer(&client, 2).await.unwrap();
            if completed.fetch_add(1, Ordering::SeqCst) == SENDERS - 1
                && let Some(tx) = done_tx.lock().unwrap().take()
            {
                let _ = tx.send(());
            }
        }));
    }

    assert_eq!(block_on(done_rx), Ok(()));
    assert_eq!(completed.load(Ordering::SeqCst), SENDERS);

    executor.shutdown();
    block_on(client.close()).unwrap();
}
