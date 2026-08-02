use std::os::unix::net::UnixDatagram;
use std::sync::Arc;
use std::time::Duration;

use autd3_rs_link_remote::{Actual, BusSnapshot, Desired, SharedBus};

fn socket_path() -> Option<String> {
    let path = std::env::var("NOTIFY_SOCKET").ok()?;
    (!path.is_empty()).then_some(path)
}

fn send(message: &str) -> Option<std::io::Result<()>> {
    let path = socket_path()?;
    Some(send_to(&path, message))
}

fn send_to(path: &str, message: &str) -> std::io::Result<()> {
    let socket = UnixDatagram::unbound()?;
    let address = if let Some(rest) = path.strip_prefix('@') {
        format!("\0{rest}")
    } else {
        path.to_owned()
    };
    socket.send_to(message.as_bytes(), address).map(|_| ())
}

pub fn ready() {
    match send("READY=1") {
        None => {}
        Some(Ok(())) => tracing::debug!("told systemd the server is ready"),
        Some(Err(e)) => tracing::warn!(
            error = %e,
            "failed to tell systemd the server is ready; it will be killed once \
             TimeoutStartSec expires",
        ),
    }
}

fn watchdog_period() -> Option<Duration> {
    let usec: u64 = std::env::var("WATCHDOG_USEC").ok()?.parse().ok()?;
    ping_period(Duration::from_micros(usec))
}

fn ping_period(timeout: Duration) -> Option<Duration> {
    let half = timeout / 2;
    (!half.is_zero()).then_some(half)
}

fn driving(snapshot: &BusSnapshot) -> bool {
    snapshot.desired == Desired::Open && snapshot.actual == Actual::Open
}

pub fn spawn_watchdog(bus: Arc<SharedBus>) {
    let Some(period) = watchdog_period() else {
        return;
    };
    if std::thread::Builder::new()
        .name("autd3-remote-watchdog".to_owned())
        .spawn(move || {
            let mut last_exchanges = None;
            loop {
                std::thread::sleep(period);
                let snapshot = bus.snapshot();
                if snapshot.stopped {
                    tracing::error!("the bus loop is gone; letting systemd restart the server");
                    continue;
                }
                if !driving(&snapshot) {
                    last_exchanges = None;
                    let _ = send("WATCHDOG=1");
                    continue;
                }
                if last_exchanges.replace(snapshot.exchanges) == Some(snapshot.exchanges) {
                    tracing::error!(
                        exchanges = snapshot.exchanges,
                        "the bus is open but has not exchanged a frame since the last watchdog \
                         ping; letting systemd restart the server",
                    );
                    continue;
                }
                let _ = send("WATCHDOG=1");
            }
        })
        .is_err()
    {
        tracing::warn!("failed to spawn the watchdog thread; systemd will restart the server");
        return;
    }
    tracing::info!(?period, "pinging the systemd watchdog");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ping_never_falls_on_the_far_side_of_the_watchdog_timeout() {
        for secs in [1, 2, 15, 30, 120] {
            let timeout = Duration::from_secs(secs);
            let period = ping_period(timeout).unwrap();
            assert!(
                period * 2 <= timeout,
                "a {timeout:?} watchdog pinged every {period:?}",
            );
        }
        assert_eq!(
            ping_period(Duration::from_secs(30)),
            Some(Duration::from_secs(15))
        );
        assert_eq!(ping_period(Duration::ZERO), None);
    }
}
