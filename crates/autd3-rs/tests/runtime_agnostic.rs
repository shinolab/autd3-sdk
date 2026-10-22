mod common;

use autd3_rs::commands::{ForceFan, Nop, SetSilencer};
use autd3_rs::{Client, Error};
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use pollster::block_on;

use common::{emulator, geometry, open_with, option};

fn open(emulator: &UdpEmulator) -> Client {
    open_with(emulator, &option(emulator))
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
        let client = Client::open(&geometry(2), &option(&emulator)).unwrap();
        assert_eq!(client.num_devices(), 2);
        stream_silencer(&client, 4).await.unwrap();
        client.send(ForceFan { value: true }).await.unwrap();
        client.send((Nop, Nop, Nop)).await.unwrap();
        assert_eq!(client.read_firmware_version().await.unwrap().len(), 2);
        client.close().unwrap();
        assert!(matches!(client.close(), Err(Error::Closed)));
    });
    assert!(emulator.with_devices(|devices| devices.iter().all(|d| d.fpga().force_fan())));
}

#[test]
#[ignore = "depends on real time"]
fn a_timeout_is_noticed_without_an_async_runtime() {
    let emulator = emulator(1);
    let client = open(&emulator);

    emulator.drop_next_replies(0, 1);
    assert!(matches!(
        block_on(client.send(Nop)),
        Err(Error::Timeout { .. })
    ));
    block_on(client.send(Nop)).unwrap();

    client.close().unwrap();
}

#[test]
#[ignore = "depends on real time"]
fn more_concurrent_sends_than_slots_all_complete() {
    const SENDERS: usize = 12;
    let emulator = emulator(1);
    let client = open(&emulator);
    let before = client.bus_stats().frames();

    let outcomes: Vec<Result<(), Error>> = std::thread::scope(|scope| {
        let senders: Vec<_> = (0..SENDERS)
            .map(|_| scope.spawn(|| block_on(stream_silencer(&client, 2))))
            .collect();
        senders
            .into_iter()
            .map(|sender| sender.join().unwrap())
            .collect()
    });

    assert_eq!(outcomes.len(), SENDERS);
    assert!(outcomes.iter().all(Result::is_ok));
    assert_eq!(
        client.bus_stats().frames() - before,
        u64::try_from(SENDERS * 2).unwrap()
    );

    client.close().unwrap();
}

#[test]
#[ignore = "depends on real time"]
fn several_os_threads_share_one_client() {
    const THREADS: usize = 4;
    const ROUNDS: usize = 8;
    let emulator = emulator(2);
    let client = open(&emulator);
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
    client.close().unwrap();
}
