mod common;

use autd3_rs::commands::{ForceFan, Nop, SetSilencer};
use autd3_rs::{Client, Error};
use pollster::block_on;

use common::{emulator, geometry, option};

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
