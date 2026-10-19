use std::net::{Ipv6Addr, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use autd3_rs::commands::{Modulation, Nop};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::udp::UdpOption;
use autd3_rs::value::SamplingConfig;
use autd3_rs::{
    Client, ClientConfig, DeviceState, Error, Frames, LinkError, MAX_INFLIGHT, ServerLink,
};
use autd3_rs_firmware_emulator::test_utils::FpgaEmulatorTestExt;
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use autd3_rs_server::{Phase, Server, ServerOption, Status, UnitState};

const PATIENCE: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(10);
const MONITOR_INTERVAL: Duration = Duration::from_millis(50);
const SYNC_TIMEOUT: Duration = Duration::from_millis(300);
const LOST_TIMEOUT: Duration = Duration::from_millis(200);
const RECOVERY_INTERVAL: Duration = Duration::from_millis(20);
const ACK_TIMEOUT: Duration = Duration::from_millis(250);
const ADDR_SYNC_TIME_0: u16 = 0x10;

struct Harness {
    emulator: UdpEmulator,
    server: Arc<Server>,
    stop: Arc<AtomicBool>,
    statuses: Arc<Mutex<Vec<Status>>>,
    thread: Option<JoinHandle<()>>,
}

fn wait_until(what: &str, mut holds: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !holds() {
        assert!(Instant::now() < deadline, "timed out waiting until {what}");
        std::thread::sleep(POLL);
    }
}

fn geometry(num_devices: usize) -> Geometry {
    Geometry::new((0..num_devices).map(|_| Autd3::default()))
}

fn config() -> ClientConfig {
    ClientConfig {
        ack_timeout: ACK_TIMEOUT,
        ..ClientConfig::default()
    }
}

fn link_error(error: &Error) -> Option<&LinkError> {
    match error {
        Error::Network(cause) => (**cause).downcast_ref::<LinkError>(),
        _ => None,
    }
}

impl Harness {
    fn start_with(emulator: UdpEmulator, transport: impl FnOnce(UdpOption) -> UdpOption) -> Self {
        let server = Arc::new(
            Server::bind(ServerOption {
                transport: transport(UdpOption {
                    sync_timeout: SYNC_TIMEOUT,
                    lost_timeout: LOST_TIMEOUT,
                    ..UdpOption::for_emulator(emulator.interface())
                }),
                listen: "[::1]:0".parse().unwrap(),
                monitor_interval: MONITOR_INTERVAL,
                retry_interval: MONITOR_INTERVAL,
                ..ServerOption::default()
            })
            .unwrap(),
        );
        let stop = Arc::new(AtomicBool::new(false));
        let statuses = Arc::new(Mutex::new(Vec::new()));
        let thread = {
            let server = Arc::clone(&server);
            let stop = Arc::clone(&stop);
            let statuses = Arc::clone(&statuses);
            std::thread::spawn(move || {
                server.run(&stop, |status| {
                    statuses
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(status.clone());
                });
            })
        };
        Self {
            emulator,
            server,
            stop,
            statuses,
            thread: Some(thread),
        }
    }

    fn start(emulator: UdpEmulator) -> Self {
        Self::start_with(emulator, |option| option)
    }

    fn serving_with(num_devices: usize, transport: impl FnOnce(UdpOption) -> UdpOption) -> Self {
        let harness = Self::start_with(UdpEmulator::spawn(num_devices).unwrap(), transport);
        harness.wait_for("the server serves the devices", |status| {
            status.phase == Phase::Serving
        });
        harness
    }

    fn serving(num_devices: usize) -> Self {
        Self::serving_with(num_devices, |option| option)
    }

    fn last_status(&self) -> Option<Status> {
        self.statuses
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .last()
            .cloned()
    }

    fn wait_for(&self, what: &str, holds: impl Fn(&Status) -> bool) {
        wait_until(what, || {
            self.last_status().is_some_and(|status| holds(&status))
        });
    }

    fn try_open_as(&self, num_devices: usize) -> Result<Client, Error> {
        pollster::block_on(Client::open(
            &geometry(num_devices),
            self.server.link(),
            config(),
        ))
    }

    fn try_open(&self) -> Result<Client, Error> {
        self.try_open_as(self.emulator.num_devices())
    }

    fn open(&self) -> Client {
        self.try_open().unwrap()
    }

    fn id_resets(&self) -> Vec<u64> {
        (0..self.emulator.num_devices())
            .map(|device| self.emulator.id_resets_received(device))
            .collect()
    }

    fn heartbeats(&self) -> Vec<u64> {
        (0..self.emulator.num_devices())
            .map(|device| self.emulator.heartbeats_received(device))
            .collect()
    }

    fn sync_times(&self) -> Vec<Vec<u16>> {
        self.emulator.with_devices(|devices| {
            devices
                .iter()
                .map(|d| {
                    (0..4)
                        .map(|i| d.fpga().controller_reg(ADDR_SYNC_TIME_0 + i))
                        .collect()
                })
                .collect()
        })
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn states(client: &Client) -> Vec<DeviceState> {
    client.state_checker().check().unwrap().devices().to_vec()
}

fn rejection(result: Result<Client, Error>) -> String {
    let error = result.err().expect("the server should refuse the client");
    match link_error(&error) {
        Some(LinkError::Rejected(reason)) => reason.clone(),
        other => panic!("expected a rejection, got {other:?} ({error})"),
    }
}

#[test]
#[ignore = "depends on real time"]
fn a_client_reaches_the_devices_through_the_server() {
    let harness = Harness::serving(3);
    let client = harness.open();

    assert_eq!(client.num_devices(), 3);
    pollster::block_on(client.send(Nop)).unwrap();
    assert_eq!(
        pollster::block_on(client.read_firmware_version())
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        pollster::block_on(client.read_fpga_state()).unwrap().len(),
        3
    );
    assert_eq!(
        pollster::block_on(client.read_telemetry()).unwrap().len(),
        3
    );
    assert!(client.device_time_now().is_ok());
    assert_eq!(states(&client), [DeviceState::Ready; 3]);

    pollster::block_on(client.close()).unwrap();
}

#[test]
#[ignore = "depends on real time"]
fn a_streamed_command_passes_through_the_server() {
    let harness = Harness::serving(2);
    let client = harness.open();
    let data: Vec<u8> = (0..30_000u32).map(|i| i.to_le_bytes()[0] ^ 0x5A).collect();
    let command = Modulation::new(SamplingConfig::FREQ_4K, &data);
    assert!(Frames::encode(client.geometry(), command).unwrap().len() > MAX_INFLIGHT);

    let stream = pollster::block_on(client.send_streaming(command)).unwrap();
    pollster::block_on(stream).unwrap();

    assert!(client.bus_stats().frames() > MAX_INFLIGHT as u64);
    pollster::block_on(client.close()).unwrap();
}

#[test]
#[ignore = "depends on real time"]
fn streaming_many_frames_back_to_back_keeps_the_session_open() {
    let harness = Harness::serving(10);
    let client = harness.open();
    let data: Vec<u8> = (0..65_536u32)
        .map(|i| i.to_le_bytes()[0] ^ i.to_le_bytes()[1])
        .collect();
    let command = Modulation::new(SamplingConfig::FREQ_4K, &data);

    let streams = (0..4)
        .map(|_| pollster::block_on(client.send_streaming(command)).unwrap())
        .collect::<Vec<_>>();
    for stream in streams {
        pollster::block_on(stream).unwrap();
    }

    pollster::block_on(client.send(Nop)).unwrap();
    assert_eq!(states(&client), [DeviceState::Ready; 10]);
    pollster::block_on(client.close()).unwrap();
}

#[test]
#[ignore = "depends on real time"]
fn reopening_a_client_does_not_enumerate_the_devices_again() {
    let harness = Harness::serving(3);
    let enumerated = harness.id_resets();

    for _ in 0..3 {
        let client = harness.open();
        pollster::block_on(client.send(Nop)).unwrap();
        pollster::block_on(client.close()).unwrap();
    }

    assert_eq!(harness.id_resets(), enumerated);
}

#[test]
#[ignore = "depends on real time"]
fn the_server_sends_heartbeats_only_while_a_client_is_attached() {
    let harness = Harness::serving_with(2, |option| UdpOption {
        heartbeat: Some(Duration::from_millis(10)),
        ..option
    });
    std::thread::sleep(MONITOR_INTERVAL * 4);
    assert_eq!(harness.heartbeats(), [0, 0]);

    let client = harness.open();
    wait_until("every device heard a heartbeat", || {
        harness.heartbeats().iter().all(|&heard| heard > 0)
    });
    harness.wait_for("the status names the client", |status| {
        status.client.is_some() && status.bus.heartbeats > 0
    });

    drop(client);
    harness.wait_for("the client is gone from the status", |status| {
        status.client.is_none()
    });
    let after_drop = harness.heartbeats();
    std::thread::sleep(MONITOR_INTERVAL * 4);
    assert_eq!(harness.heartbeats(), after_drop);
}

#[test]
#[ignore = "depends on real time"]
fn a_device_that_answers_again_is_recovered_by_the_server_and_restored_by_the_client() {
    let harness = Harness::serving_with(3, |option| UdpOption {
        heartbeat: Some(Duration::from_millis(10)),
        recovery_interval: Some(RECOVERY_INTERVAL),
        ..option
    });
    let client = harness.open();
    let before = harness.sync_times();

    harness.emulator.set_muted(1, true);
    wait_until("the client sees the device as lost", || {
        states(&client)[1] == DeviceState::Lost
    });
    harness.emulator.set_muted(1, false);
    wait_until("the client sees every device ready", || {
        states(&client) == [DeviceState::Ready; 3]
    });
    pollster::block_on(client.send(Nop)).unwrap();

    let after = harness.sync_times();
    assert_eq!(after[0], before[0]);
    assert_ne!(after[1], before[1]);
    assert_eq!(after[2], before[2]);
    assert_eq!(client.bus_stats().recoveries(), 1);
    harness.wait_for("the status counts the recovery", |status| {
        status.bus.recoveries == 1
    });
    pollster::block_on(client.close()).unwrap();
}

#[test]
#[ignore = "depends on real time"]
fn a_device_that_dropped_out_is_lost_for_the_client_and_enumerated_again_once_it_leaves() {
    let harness = Harness::serving_with(3, |option| UdpOption {
        heartbeat: Some(Duration::from_millis(10)),
        ..option
    });
    let enumerated = harness.id_resets();
    let client = harness.open();

    harness.emulator.reboot(2);

    wait_until("the client sees the device as lost", || {
        states(&client)[2] == DeviceState::Lost
    });
    harness.wait_for("the status shows the device as lost", |status| {
        status.units.len() == 3 && status.units[2].state == Some(UnitState::Lost)
    });
    assert_eq!(harness.id_resets(), enumerated);

    drop(client);
    wait_until("the server enumerated the devices again", || {
        harness
            .id_resets()
            .iter()
            .zip(&enumerated)
            .all(|(now, before)| now > before)
    });
    harness.wait_for("every device answers again", |status| {
        status.phase == Phase::Serving
            && status.units.len() == 3
            && status.units.iter().all(|unit| unit.responding)
    });
    let client = harness.open();
    pollster::block_on(client.send(Nop)).unwrap();
    pollster::block_on(client.close()).unwrap();
}

#[test]
#[ignore = "depends on real time"]
fn the_server_refuses_a_client_until_the_devices_synchronize() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    emulator.set_ptp_lock_blocked(1, true);
    let harness = Harness::start(emulator);

    harness.wait_for("the server reports why it is not serving", |status| {
        status.phase == Phase::Opening && status.error.is_some()
    });
    assert!(rejection(harness.try_open()).contains("enumerating"));

    harness.emulator.set_ptp_lock_blocked(1, false);
    harness.wait_for("the server serves the devices", |status| {
        status.phase == Phase::Serving && status.units.iter().all(|unit| unit.sync_ready)
    });
    let client = harness.open();
    pollster::block_on(client.send(Nop)).unwrap();
    pollster::block_on(client.close()).unwrap();
}

#[test]
#[ignore = "depends on real time"]
fn the_server_refuses_a_second_client_and_a_geometry_of_another_size() {
    let harness = Harness::serving(2);
    assert!(rejection(harness.try_open_as(3)).contains("3 devices"));

    let client = harness.open();
    assert!(rejection(harness.try_open()).contains("another client"));

    pollster::block_on(client.close()).unwrap();
    wait_until("the server takes a new client", || {
        harness.try_open().is_ok()
    });
}

#[test]
#[ignore = "depends on real time"]
fn a_client_without_a_server_fails_at_once() {
    let port = TcpListener::bind((Ipv6Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let error = pollster::block_on(Client::open(
        &geometry(1),
        ServerLink {
            port,
            ..ServerLink::default()
        },
        config(),
    ))
    .err()
    .unwrap();
    assert!(matches!(
        link_error(&error),
        Some(LinkError::ServerNotFound(_))
    ));
}
