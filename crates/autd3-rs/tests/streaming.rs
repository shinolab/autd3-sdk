mod common;

use std::collections::HashMap;

use autd3_rs::commands::Nop;
use autd3_rs::{Error, ExecAt, Frames, Response};

use common::{INVALID_PAYLOAD, PATIENCE, REJECTED_BY_THE_DEVICE, open};

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_rejected_frame_sent_without_waiting_fails_only_its_own_response() {
    let (_emulator, client) = open(2).await;
    let nop = Frames::encode(client.geometry(), Nop).unwrap();
    let rejected = Frames::encode(client.geometry(), REJECTED_BY_THE_DEVICE).unwrap();
    let frames = [&nop, &rejected, &nop, &nop].map(|frames| frames.frame(0).unwrap());

    let mut msg_ids = Vec::with_capacity(frames.len());
    for frame in frames {
        msg_ids.push(client.send_frame(frame, ExecAt::Immediate).await.unwrap());
    }
    let num_devices = client.num_devices();
    let mut error_codes: HashMap<u16, Vec<u8>> = msg_ids
        .iter()
        .map(|&msg_id| (msg_id, vec![0; num_devices]))
        .collect();
    let mut unanswered = msg_ids.len() * num_devices;
    tokio::time::timeout(PATIENCE, async {
        while unanswered > 0 {
            let reply = client.recv().await.unwrap();
            if let Some(codes) = error_codes.get_mut(&reply.msg_id) {
                codes[reply.device] = reply.status;
                unanswered -= 1;
            }
        }
    })
    .await
    .unwrap();
    let outcomes: Vec<_> = msg_ids
        .iter()
        .map(|msg_id| Response::from_error_codes(&error_codes[msg_id]).check())
        .collect();

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
