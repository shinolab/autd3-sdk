mod common;

use autd3_rs::commands::Nop;
use autd3_rs::{Client, Telemetry};

use common::open;

async fn telemetry(client: &Client) -> Vec<Telemetry> {
    client.read_telemetry().await.unwrap()
}

#[tokio::test]
async fn a_frame_delivered_twice_is_executed_once() {
    let (emulator, client) = open(2).await;
    let stats = client.bus_stats();

    let baseline = telemetry(&client).await;
    client.send(Nop).await.unwrap();
    let undisturbed = telemetry(&client).await;
    emulator.duplicate_next_frames(0, 1);
    client.send(Nop).await.unwrap();
    let duplicated = telemetry(&client).await;

    for device in 0..2 {
        assert_eq!(
            duplicated[device].processed - undisturbed[device].processed,
            undisturbed[device].processed - baseline[device].processed
        );
        assert_eq!(duplicated[device].seq_mismatch, 0);
    }
    assert_eq!(undisturbed[0].dedup, 0);
    assert_eq!(duplicated[0].dedup, 1);
    assert_eq!(duplicated[1].dedup, 0);
    assert_eq!(stats.resets(), 1);
    client.close().await.unwrap();
}
