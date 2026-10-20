use core::time::Duration;
use std::time::Instant;

use anyhow::{Context, Result, bail};

use autd3_rs::protocol::{Cmd, Seq};
use autd3_rs::{DeviceState, TransportOption, UdpBus};

use crate::{Link, Session, describe};

const RESET_ACK: u8 = 0xFF;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);
const RETRANSMIT_INTERVAL: Duration = Duration::from_millis(20);
const GONE_WINDOW: Duration = Duration::from_secs(5);
const GONE_LOST_TIMEOUT: Duration = Duration::from_millis(100);
const HEARTBEATS_UNTIL_LOST: u32 = 10;
const POLL: Duration = Duration::from_millis(10);

fn verdict(ok: bool) -> &'static str {
    if ok { "[OK]" } else { "[FAIL]" }
}

fn not_before(msg_id: u16, first: u16) -> bool {
    msg_id.wrapping_sub(first) < 0x8000
}

fn request(bus: &UdpBus, cmd: Cmd, seq: Seq, ack: u8) -> Result<()> {
    let frame = [seq.get(), cmd.as_u8(), 0, 0, 0, 0];
    let mut status: Vec<Option<u8>> = vec![None; bus.num_devices()];
    let first = bus.peek_next_msg_id();
    let end = Instant::now() + REQUEST_TIMEOUT;
    while Instant::now() < end {
        bus.broadcast(bus.reserve_msg_id(), &frame)?;
        let retransmit_at = (Instant::now() + RETRANSMIT_INTERVAL).min(end);
        while let Some(reply) = bus.recv(retransmit_at)? {
            if not_before(reply.msg_id, first) && reply.acked_seq == ack {
                status[reply.device].get_or_insert(reply.error_code);
            }
            if status.iter().all(Option::is_some) {
                break;
            }
        }
        if status.iter().all(Option::is_some) {
            break;
        }
    }
    for (device, status) in status.iter().enumerate() {
        match status {
            None => {
                bail!("device[{device}] did not acknowledge {cmd:?} within {REQUEST_TIMEOUT:?}")
            }
            Some(0) => {}
            Some(code) => bail!("device[{device}] rejected {cmd:?} with code {code:#04x}"),
        }
    }
    Ok(())
}

fn gone(bus: &UdpBus) -> Result<(bool, Vec<DeviceState>)> {
    let states = bus.state_checker();
    let started = Instant::now();
    loop {
        bus.send_heartbeat()?;
        while bus.recv(Instant::now() + POLL)?.is_some() {}
        let seen = states.check()?.devices().to_vec();
        let all_lost = seen.iter().all(|d| *d == DeviceState::Lost);
        if all_lost || started.elapsed() >= GONE_WINDOW {
            return Ok((all_lost, seen));
        }
    }
}

fn reboot_all(
    option: &TransportOption,
    heartbeat: Duration,
    devices: usize,
) -> Result<(bool, Vec<DeviceState>)> {
    let option = TransportOption {
        lost_timeout: GONE_LOST_TIMEOUT.max(heartbeat.saturating_mul(HEARTBEATS_UNTIL_LOST)),
        ..option.clone()
    };
    let bus = UdpBus::open_unsynchronized(&option, devices).context("enumerating the devices")?;
    request(&bus, Cmd::Reset, Seq::ZERO, RESET_ACK)?;
    request(&bus, Cmd::Reboot, Seq::ZERO, Seq::ZERO.get())?;
    let outcome = gone(&bus);
    bus.close()?;
    outcome
}

pub(crate) async fn run(client: Session, link: &Link<'_>) -> Result<Session> {
    let devices = client.num_devices();
    let before = client.read_firmware_version().await;
    if let Err(e) = &before {
        println!("  reading the firmware version before the reboot failed: {e}");
    }
    if let Err(e) = client.close().await {
        println!("  closing the client before the reboot reported: {e}");
    }
    drop(client);

    let option = link.option.clone();
    let heartbeat = link.heartbeat;
    let started = Instant::now();
    match tokio::task::spawn_blocking(move || reboot_all(&option, heartbeat, devices)).await? {
        Ok((all_lost, seen)) => {
            println!("  [OK] every device acknowledged Reboot");
            println!(
                "  {} every device stopped answering under its unit ID within {GONE_WINDOW:?} of the acknowledgement: {seen:?}",
                verdict(all_lost)
            );
        }
        Err(e) => println!("  [FAIL] Reboot was not delivered to every device: {e:#}"),
    }

    let client = link.reopen().await?;
    let states = client
        .state_checker()
        .check()
        .map(|s| s.devices().to_vec())
        .unwrap_or_default();
    println!(
        "  {} the devices enumerate and synchronise again {:.1} s after the Reboot request: {states:?}",
        verdict(states.len() == devices && states.iter().all(|d| *d == DeviceState::Ready)),
        started.elapsed().as_secs_f32()
    );

    match (before, client.read_firmware_version().await) {
        (Ok(before), Ok(after)) => println!(
            "  {} every device runs the same firmware as before the reboot",
            verdict(before.len() == after.len() && before.iter().zip(&after).all(|(b, a)| b == a))
        ),
        (_, Err(e)) => println!("  [FAIL] reading the firmware version after the reboot: {e}"),
        (Err(_), Ok(_)) => {}
    }
    match client.read_telemetry().await {
        Ok(telemetry) => {
            let failures: Vec<u8> = telemetry.iter().map(|t| t.boot_failure).collect();
            println!(
                "  {} no device reports a boot failure: {failures:?}",
                verdict(failures.iter().all(|c| *c == 0))
            );
        }
        Err(e) => println!("  [FAIL] reading the telemetry after the reboot: {e}"),
    }
    describe(&client).await;
    Ok(client)
}
