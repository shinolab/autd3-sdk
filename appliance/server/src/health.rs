use std::sync::Arc;
use std::time::Duration;

use autd3_rs_core::DeviceState;
use autd3_rs_link_remote::{Actual, BusSnapshot, SharedBus};

pub fn spawn(bus: Arc<SharedBus>, interval: Duration) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("autd3-remote-health".to_owned())
        .spawn(move || {
            let mut last = Reported::default();
            loop {
                std::thread::sleep(interval);
                let snapshot = bus.snapshot();
                report(&snapshot, &mut last);
            }
        })?;
    Ok(())
}

#[derive(Default)]
struct Reported {
    actual: Option<Actual>,
    exchanges: Option<u64>,
}

fn report(snapshot: &BusSnapshot, last: &mut Reported) {
    if snapshot.stopped {
        tracing::warn!("the bus loop is gone; the server cannot drive devices again");
    }
    if snapshot.actual != Actual::Open {
        last.exchanges = None;
        if last.actual.as_ref() != Some(&snapshot.actual) {
            tracing::info!(
                desired = ?snapshot.desired,
                actual = %snapshot.actual,
                "the bus is not driving devices",
            );
            last.actual = Some(snapshot.actual.clone());
        }
        return;
    }
    last.actual = Some(Actual::Open);

    if last.exchanges.replace(snapshot.exchanges) == Some(snapshot.exchanges) {
        tracing::warn!(
            exchanges = snapshot.exchanges,
            "the bus reads as open but has not exchanged a frame since the last report; \
             the bus loop is stuck",
        );
    }

    let all_op = snapshot
        .devices
        .iter()
        .all(|state| *state == DeviceState::Op);
    if all_op {
        tracing::info!(
            devices = snapshot.devices.len(),
            recoveries = snapshot.recoveries,
            stale_cycles = snapshot.stale_cycles,
            lost_cycles = snapshot.lost_cycles,
            phase_excursions = snapshot.phase_excursions,
            worst_phase_deviation_ns = snapshot.worst_phase_deviation_ns,
            "bus healthy: every device is in OP",
        );
    } else {
        let device_states = snapshot
            .devices
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        tracing::warn!(
            device_states,
            recoveries = snapshot.recoveries,
            stale_cycles = snapshot.stale_cycles,
            lost_cycles = snapshot.lost_cycles,
            phase_excursions = snapshot.phase_excursions,
            worst_phase_deviation_ns = snapshot.worst_phase_deviation_ns,
            "bus degraded: not every device is in OP",
        );
    }
}
