mod common;

use autd3_rs::commands::Nop;
use autd3_rs::{Error, Frames};

use common::{INVALID_PAYLOAD, REJECTED_BY_THE_DEVICE, open};

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_rejected_frame_sent_without_waiting_fails_only_its_own_response() {
    let (_emulator, client) = open(2);
    let nop = Frames::encode(client.geometry(), Nop).unwrap();
    let rejected = Frames::encode(client.geometry(), REJECTED_BY_THE_DEVICE).unwrap();
    let frames = [&nop, &rejected, &nop, &nop].map(|frames| frames.frame(0).unwrap());

    let mut responses = Vec::with_capacity(frames.len());
    for frame in frames {
        responses.push(client.send_frame(frame).await.unwrap());
    }
    let mut outcomes = Vec::with_capacity(responses.len());
    for response in responses {
        outcomes.push(response.await.unwrap().check());
    }

    assert!(outcomes[0].is_ok());
    assert!(matches!(
        outcomes[1],
        Err(Error::DeviceError {
            device: 0,
            code: INVALID_PAYLOAD
        })
    ));
    assert!(outcomes[2..].iter().all(Result::is_ok));
    client.close().unwrap();
}
