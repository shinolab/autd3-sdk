mod common;

use autd3_rs::commands::Nop;
use autd3_rs::{Error, ExecAt, Frames, InflightFrames};

use common::{INVALID_PAYLOAD, PATIENCE, REJECTED_BY_THE_DEVICE, open};

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_rejected_frame_sent_without_waiting_fails_only_its_own_response() {
    let (_emulator, client) = open(2).await;
    let nop = Frames::encode(client.geometry(), Nop).unwrap();
    let rejected = Frames::encode(client.geometry(), REJECTED_BY_THE_DEVICE).unwrap();
    let frames = [&nop, &rejected, &nop, &nop].map(|frames| frames.frame(0).unwrap());

    let mut inflight = InflightFrames::default();
    for frame in frames {
        inflight.push(client.send_frame(frame, ExecAt::Immediate).await.unwrap());
    }
    let mut outcomes = Vec::with_capacity(inflight.len());
    tokio::time::timeout(PATIENCE, async {
        while let Some(response) = inflight.collect_oldest(&client).await.unwrap() {
            outcomes.push(response.check());
        }
    })
    .await
    .unwrap();

    assert!(outcomes[0].is_ok());
    assert!(matches!(
        outcomes[1],
        Err(Error::DeviceError {
            device: 0,
            code: INVALID_PAYLOAD
        })
    ));
    assert!(outcomes[2..].iter().all(Result::is_ok));
}
