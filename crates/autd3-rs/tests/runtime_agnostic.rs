mod common;

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::mpsc;

use autd3_rs::commands::{ForceFan, Nop, SetSilencer};
use autd3_rs::rt::Executor;
use autd3_rs::{Client, ClientConfig, Error};
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use pollster::block_on;

use common::{config, emulator, geometry, option};

fn open(emulator: &UdpEmulator, config: ClientConfig) -> Client {
    block_on(Client::open(
        &geometry(emulator.num_devices()),
        &option(emulator),
        config,
    ))
    .unwrap()
}

async fn stream_silencer(client: &Client, rounds: usize) -> Result<(), Error> {
    for _ in 0..rounds {
        client.send(SetSilencer::default()).await?;
    }
    Ok(())
}

#[test]
#[ignore = "depends on real time"]
fn a_client_opens_sends_and_closes_without_an_async_runtime() {
    let emulator = emulator(2);
    block_on(async {
        let client = Client::open(&geometry(2), &option(&emulator), config())
            .await
            .unwrap();
        assert_eq!(client.num_devices(), 2);
        stream_silencer(&client, 4).await.unwrap();
        client.send(ForceFan { value: true }).await.unwrap();
        let stream = client.send_streaming((Nop, Nop, Nop)).await.unwrap();
        stream.await.unwrap();
        assert_eq!(client.read_firmware_version().await.unwrap().len(), 2);
        client.silent_stop().await.unwrap();
        client.close().await.unwrap();
        assert!(client.close().await.is_ok());
    });
    assert!(emulator.with_devices(|devices| devices.iter().all(|d| d.fpga().force_fan())));
}

#[test]
#[ignore = "depends on real time"]
fn a_timeout_is_noticed_without_an_async_runtime() {
    let emulator = emulator(1);
    let client = open(&emulator, config());

    emulator.drop_next_replies(0, 1);
    assert!(matches!(
        block_on(client.send(Nop)),
        Err(Error::Timeout { .. })
    ));
    block_on(client.send(Nop)).unwrap();

    block_on(client.close()).unwrap();
}

#[test]
#[ignore = "depends on real time"]
fn more_concurrent_sends_than_slots_all_complete_on_one_executor_thread() {
    const SENDERS: usize = 12;
    let emulator = emulator(1);
    let client = Arc::new(open(
        &emulator,
        ClientConfig {
            max_inflight: NonZeroUsize::new(3).unwrap(),
            ..config()
        },
    ));
    let before = client.bus_stats().frames();

    let executor = Executor::new();
    let (done_tx, done_rx) = mpsc::channel::<Result<(), Error>>();
    for _ in 0..SENDERS {
        let client = Arc::clone(&client);
        let done_tx = done_tx.clone();
        assert!(executor.spawn(async move {
            let _ = done_tx.send(stream_silencer(&client, 2).await);
        }));
    }
    drop(done_tx);

    let outcomes: Vec<_> = done_rx.iter().collect();
    assert_eq!(outcomes.len(), SENDERS);
    assert!(outcomes.iter().all(Result::is_ok));
    assert_eq!(
        client.bus_stats().frames() - before,
        u64::try_from(SENDERS * 2).unwrap()
    );

    executor.shutdown();
    block_on(client.close()).unwrap();
}

#[test]
#[ignore = "depends on real time"]
fn several_os_threads_share_one_client() {
    const THREADS: usize = 4;
    const ROUNDS: usize = 8;
    let emulator = emulator(2);
    let client = open(&emulator, config());
    let before = client.bus_stats().frames();

    std::thread::scope(|scope| {
        for _ in 0..THREADS {
            scope.spawn(|| block_on(stream_silencer(&client, ROUNDS)).unwrap());
        }
    });

    assert_eq!(
        client.bus_stats().frames() - before,
        u64::try_from(THREADS * ROUNDS).unwrap()
    );
    assert_eq!(client.bus_stats().resets(), 1);
    block_on(client.close()).unwrap();
}
