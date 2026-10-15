use std::io::{self, Write};
use std::net::{Ipv6Addr, Shutdown, SocketAddr, SocketAddrV6, TcpListener, TcpStream};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::Scope;
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::UnitInfo;
use autd3_rs::link::message::{LINK_VERSION, Notice, Outcome, Reader, Request};
use autd3_rs::udp::{Monitor, Pump, Sent, UdpBus, UdpError, UdpOption};
use autd3_rs::{DeviceState, DeviceStatus, Interface, SERVER_PORT, ServerLink};

use crate::health::{Change, Health, MISSES_BEFORE_SILENT};
use crate::session::{MsgIds, admission};
use crate::status::{BusCounters, Counters, Status};

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(1);
const WRITE_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq)]
pub struct ServerOption {
    pub transport: UdpOption,
    pub devices: Option<NonZeroUsize>,
    pub listen: SocketAddrV6,
    pub monitor_interval: Duration,
    pub retry_interval: Duration,
}

impl Default for ServerOption {
    fn default() -> Self {
        Self {
            transport: UdpOption::default(),
            devices: None,
            listen: SocketAddrV6::new(Ipv6Addr::LOCALHOST, SERVER_PORT, 0, 0),
            monitor_interval: Duration::from_secs(1),
            retry_interval: Duration::from_secs(1),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServerError {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Udp(#[from] UdpError),
    #[error("the server listens on the loopback address only, but {0} was given")]
    NotLoopback(SocketAddrV6),
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn is_timeout(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

fn pause(stop: &AtomicBool, duration: Duration) {
    let until = Instant::now() + duration;
    while !stop.load(Ordering::Acquire) {
        let remaining = until.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return;
        }
        std::thread::sleep(remaining.min(POLL_INTERVAL));
    }
}

struct Session {
    peer: SocketAddr,
    writer: Mutex<TcpStream>,
    ids: Mutex<MsgIds>,
    states: Mutex<Option<Vec<DeviceState>>>,
}

impl Session {
    fn notify(&self, notice: &Notice<'_>) {
        let mut buf = Vec::new();
        notice.encode(&mut buf);
        let mut stream = lock(&self.writer);
        if let Err(e) = stream.write_all(&buf) {
            tracing::debug!(peer = %self.peer, "writing to the client failed: {e}");
            let _ = stream.shutdown(Shutdown::Both);
        }
    }

    fn publish_states(&self, states: Vec<DeviceState>) {
        let mut last = lock(&self.states);
        if last.as_ref() != Some(&states) {
            self.notify(&Notice::States(states.clone()));
            *last = Some(states);
        }
    }

    fn close(&self) {
        let _ = lock(&self.writer).shutdown(Shutdown::Both);
    }
}

#[derive(Default)]
struct Hub {
    bus: Mutex<Option<Arc<UdpBus>>>,
    session: Mutex<Option<Arc<Session>>>,
    relayed: Mutex<Counters>,
}

impl Hub {
    fn bus(&self) -> Option<Arc<UdpBus>> {
        lock(&self.bus).clone()
    }

    fn session(&self) -> Option<Arc<Session>> {
        lock(&self.session).clone()
    }

    fn detach(&self, session: &Arc<Session>) {
        let mut slot = lock(&self.session);
        if slot
            .as_ref()
            .is_some_and(|attached| Arc::ptr_eq(attached, session))
        {
            *slot = None;
        }
    }

    fn count(&self, update: impl FnOnce(&mut Counters)) {
        update(&mut lock(&self.relayed));
    }
}

fn outcome_of(sent: Result<Sent, UdpError>) -> (Option<u128>, Option<u16>, String) {
    match sent {
        Ok(Sent {
            devices,
            unsent: None,
            ..
        }) => (Some(devices), None, String::new()),
        Ok(Sent {
            devices,
            unsent: Some(unsent),
            ..
        }) => (
            Some(devices),
            Some(u16::try_from(unsent.device).unwrap_or(u16::MAX)),
            unsent.cause.to_string(),
        ),
        Err(UdpError::Unsent { device, source }) => (
            Some(0),
            Some(u16::try_from(device).unwrap_or(u16::MAX)),
            source.to_string(),
        ),
        Err(e) => (None, None, e.to_string()),
    }
}

fn relay(hub: &Hub, session: &Session, request: Request<'_>) {
    match request {
        Request::Send {
            msg_id,
            broadcast,
            frames,
        } => {
            let sent = match hub.bus() {
                Some(bus) => {
                    let bus_msg_id = bus.reserve_msg_id();
                    lock(&session.ids).remember(bus_msg_id, msg_id);
                    match (broadcast, frames.first()) {
                        (true, Some(frame)) => bus.send_broadcast_as(bus_msg_id, frame),
                        _ => bus.send_as(bus_msg_id, &frames),
                    }
                }
                None => Err(UdpError::Closed),
            };
            let (devices, device, reason) = outcome_of(sent);
            let outcome = match (devices, device) {
                (Some(devices), None) => Outcome::Sent { devices },
                (Some(devices), Some(device)) => Outcome::Unsent {
                    devices,
                    device,
                    reason: &reason,
                },
                (None, _) => Outcome::Failed { reason: &reason },
            };
            hub.count(|relayed| {
                relayed.frames += 1;
                if !matches!(outcome, Outcome::Sent { .. }) {
                    relayed.dropped += 1;
                }
            });
            session.notify(&Notice::Settled { msg_id, outcome });
        }
        Request::Admit => {
            if let Some(bus) = hub.bus() {
                bus.admit_recovered();
            }
        }
        Request::Wake => session.notify(&Notice::Wake),
        Request::Hello { .. } => {
            tracing::debug!(peer = %session.peer, "ignoring a second hello");
        }
    }
}

fn read_session(
    hub: &Hub,
    session: &Session,
    mut stream: TcpStream,
    mut reader: Reader,
    stop: &AtomicBool,
) {
    if let Err(e) = stream.set_read_timeout(Some(POLL_INTERVAL)) {
        tracing::debug!(peer = %session.peer, "configuring the client socket failed: {e}");
        return;
    }
    while !stop.load(Ordering::Acquire) {
        loop {
            match reader.next_message() {
                Ok(Some((kind, body))) => match Request::decode(kind, body) {
                    Ok(request) => relay(hub, session, request),
                    Err(e) => {
                        tracing::warn!(peer = %session.peer, "the client sent a malformed message: {e}");
                        return;
                    }
                },
                Ok(None) => break,
                Err(e) => {
                    tracing::warn!(peer = %session.peer, "the client sent a malformed message: {e}");
                    return;
                }
            }
        }
        match reader.fill(&mut stream) {
            Ok(0) => return,
            Ok(_) => {}
            Err(e) if is_timeout(&e) => {}
            Err(e) => {
                tracing::debug!(peer = %session.peer, "reading from the client failed: {e}");
                return;
            }
        }
    }
}

fn hello(stream: &mut TcpStream, reader: &mut Reader) -> Option<(u16, u16)> {
    loop {
        match reader.next_message() {
            Ok(Some((kind, body))) => {
                return match Request::decode(kind, body) {
                    Ok(Request::Hello {
                        version,
                        num_devices,
                    }) => Some((version, num_devices)),
                    _ => None,
                };
            }
            Ok(None) => {}
            Err(_) => return None,
        }
        match reader.fill(stream) {
            Ok(0) | Err(_) => return None,
            Ok(_) => {}
        }
    }
}

fn pump(hub: &Hub, bus: &Arc<UdpBus>, stop: &AtomicBool) -> Result<(), UdpError> {
    let mut pump = Pump::new(Arc::clone(bus));
    let checker = bus.state_checker();
    let clock = bus.device_clock();
    while !stop.load(Ordering::Acquire) {
        let session = hub.session();
        let mut observed_time = false;
        while let Some(reply) = pump.recv()? {
            let Some(session) = &session else {
                continue;
            };
            let Some(msg_id) = lock(&session.ids).client_of(reply.msg_id) else {
                continue;
            };
            observed_time |= reply.device == 0;
            session.notify(&Notice::Reply {
                msg_id,
                device: u8::try_from(reply.device).unwrap_or(u8::MAX),
                ack: reply.ack,
                status: reply.status,
                data: reply.data(),
            });
            hub.count(|relayed| relayed.replies += 1);
        }
        let now = Instant::now();
        let due = pump.maintain(now, session.is_some())?;
        let recovered = pump.take_recovered();
        if let Some(session) = &session {
            if observed_time && let Some(time) = clock.now() {
                session.notify(&Notice::Clock(time.sys_time()));
            }
            if recovered != 0 {
                session.notify(&Notice::Recovered(recovered));
            }
            if let Ok(status) = checker.check() {
                session.publish_states(status.devices().to_vec());
            }
        }
        let idle = now + POLL_INTERVAL;
        bus.wait_readable(due.map_or(idle, |due| due.min(idle)))?;
    }
    Ok(())
}

fn read_all(monitor: &Monitor, num_devices: usize) -> Result<Vec<Option<UnitInfo>>, UdpError> {
    let mut devices = vec![None; num_devices];
    for _ in 0..MISSES_BEFORE_SILENT {
        for (known, read) in devices.iter_mut().zip(monitor.read()?) {
            if read.is_some() {
                *known = read;
            }
        }
        if devices.iter().all(Option::is_some) {
            break;
        }
    }
    Ok(devices)
}

fn report(changes: Vec<Change>) {
    for change in changes {
        match change {
            Change::Silent(device) => tracing::warn!(device, "a device stopped answering"),
            Change::Answering(device) => tracing::info!(device, "a device answers again"),
            Change::Unlocked(device) => tracing::warn!(device, "a device lost its PTP lock"),
            Change::Locked(device) => tracing::info!(device, "a device locked again"),
        }
    }
}

struct Watch<'a> {
    hub: &'a Hub,
    bus: &'a UdpBus,
    monitor: &'a Monitor,
    stop: &'a AtomicBool,
    ended: &'a AtomicBool,
}

pub struct Server {
    option: ServerOption,
    listener: TcpListener,
    addr: SocketAddrV6,
}

impl Server {
    pub fn bind(option: ServerOption) -> Result<Self, ServerError> {
        option.transport.validate()?;
        if option.transport.iface == Interface::Simulator {
            return Err(UdpError::UnsupportedInterface(option.transport.iface).into());
        }
        if !option.listen.ip().is_loopback() {
            return Err(ServerError::NotLoopback(option.listen));
        }
        let listener = TcpListener::bind(option.listen)?;
        listener.set_nonblocking(true)?;
        let SocketAddr::V6(addr) = listener.local_addr()? else {
            return Err(io::Error::from(io::ErrorKind::AddrNotAvailable).into());
        };
        Ok(Self {
            option,
            listener,
            addr,
        })
    }

    #[must_use]
    pub fn addr(&self) -> SocketAddrV6 {
        self.addr
    }

    #[must_use]
    pub fn link(&self) -> ServerLink {
        ServerLink {
            port: self.addr.port(),
            ..ServerLink::default()
        }
    }

    pub fn run(&self, stop: &AtomicBool, mut on_status: impl FnMut(&Status)) {
        let hub = Hub::default();
        std::thread::scope(|scope| {
            let hub = &hub;
            if let Err(e) = std::thread::Builder::new()
                .name("autd3-server-accept".into())
                .spawn_scoped(scope, move || self.accept(scope, hub, stop))
            {
                tracing::error!("starting to accept clients failed: {e}");
                return;
            }
            let mut failure: Option<String> = None;
            while !stop.load(Ordering::Acquire) {
                on_status(&Status::opening(failure.clone()));
                if failure.is_none() {
                    tracing::info!("enumerating the devices");
                }
                let served = UdpBus::open_devices(
                    &self.option.transport,
                    self.option.devices.map(NonZeroUsize::get),
                )
                .map_err(ServerError::from)
                .and_then(|bus| self.serve(hub, &Arc::new(bus), stop, &mut on_status));
                match served {
                    Ok(()) => failure = None,
                    Err(e) => {
                        let reason = e.to_string();
                        if failure.as_ref() == Some(&reason) {
                            tracing::debug!("still could not serve the devices: {reason}");
                        } else {
                            tracing::warn!("could not serve the devices; retrying: {reason}");
                        }
                        failure = Some(reason);
                        pause(stop, self.option.retry_interval);
                    }
                }
            }
        });
    }

    fn accept<'scope, 'env>(
        &'env self,
        scope: &'scope Scope<'scope, 'env>,
        hub: &'env Hub,
        stop: &'env AtomicBool,
    ) {
        while !stop.load(Ordering::Acquire) {
            match self.listener.accept() {
                Ok((stream, peer)) => {
                    let Some((session, stream, reader)) = Self::handshake(hub, stream, peer) else {
                        continue;
                    };
                    let spawned = std::thread::Builder::new()
                        .name("autd3-server-session".into())
                        .spawn_scoped(scope, {
                            let session = Arc::clone(&session);
                            move || {
                                read_session(hub, &session, stream, reader, stop);
                                hub.detach(&session);
                                session.close();
                                tracing::info!(peer = %session.peer, "the client detached");
                            }
                        });
                    if let Err(e) = spawned {
                        tracing::error!("starting a session failed: {e}");
                        hub.detach(&session);
                        session.close();
                    }
                }
                Err(e) if is_timeout(&e) => std::thread::sleep(POLL_INTERVAL),
                Err(e) => {
                    tracing::debug!("accepting a client failed: {e}");
                    std::thread::sleep(POLL_INTERVAL);
                }
            }
        }
    }

    fn handshake(
        hub: &Hub,
        mut stream: TcpStream,
        peer: SocketAddr,
    ) -> Option<(Arc<Session>, TcpStream, Reader)> {
        let configured = stream
            .set_nonblocking(false)
            .and_then(|()| stream.set_nodelay(true))
            .and_then(|()| stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT)))
            .and_then(|()| stream.set_write_timeout(Some(WRITE_TIMEOUT)));
        if let Err(e) = configured {
            tracing::debug!(%peer, "configuring a client socket failed: {e}");
            return None;
        }
        let mut reader = Reader::new();
        let Some((version, num_devices)) = hello(&mut stream, &mut reader) else {
            tracing::debug!(%peer, "a client connected without a hello");
            return None;
        };
        let writer = stream.try_clone().ok()?;
        let session = Arc::new(Session {
            peer,
            writer: Mutex::new(writer),
            ids: Mutex::new(MsgIds::default()),
            states: Mutex::new(None),
        });
        let bus = hub.bus();
        let mut slot = lock(&hub.session);
        let admitted = admission(
            version,
            num_devices,
            bus.as_ref().map(|bus| bus.num_devices()),
            slot.is_some(),
        );
        let bus = match (admitted, bus) {
            (Ok(()), Some(bus)) => bus,
            (Err(reason), _) => {
                tracing::info!(%peer, %reason, "refused a client");
                session.notify(&Notice::Reject {
                    version: LINK_VERSION,
                    reason: &reason,
                });
                session.close();
                return None;
            }
            (Ok(()), None) => return None,
        };
        session.notify(&Notice::Welcome {
            version: LINK_VERSION,
            num_devices: u16::try_from(bus.num_devices()).unwrap_or(u16::MAX),
        });
        if let Ok(status) = bus.state_checker().check() {
            session.publish_states(status.devices().to_vec());
        }
        *slot = Some(Arc::clone(&session));
        drop(slot);
        bus.wake();
        tracing::info!(%peer, "a client attached");
        Some((session, stream, reader))
    }

    fn serve(
        &self,
        hub: &Hub,
        bus: &Arc<UdpBus>,
        stop: &AtomicBool,
        on_status: &mut impl FnMut(&Status),
    ) -> Result<(), ServerError> {
        let num_devices = bus.num_devices();
        let monitor = Monitor::new(bus)?;
        let mut health = Health::new(read_all(&monitor, num_devices)?);
        *lock(&hub.relayed) = Counters::default();
        *lock(&hub.bus) = Some(Arc::clone(bus));
        tracing::info!(num_devices, addr = %self.addr, "serving the devices");
        let ended = AtomicBool::new(false);
        let outcome = std::thread::scope(|scope| {
            let pumping = std::thread::Builder::new()
                .name("autd3-server-bus".into())
                .spawn_scoped(scope, || {
                    let pumped = pump(hub, bus, &ended);
                    if let Err(e) = &pumped {
                        tracing::error!("driving the devices failed: {e}");
                    }
                    ended.store(true, Ordering::Release);
                    pumped
                })?;
            let watch = Watch {
                hub,
                bus,
                monitor: &monitor,
                stop,
                ended: &ended,
            };
            let watched = self.watch(&watch, &mut health, on_status);
            ended.store(true, Ordering::Release);
            let pumped = pumping.join().unwrap_or(Ok(()));
            watched.and(pumped.map_err(ServerError::from))
        });
        *lock(&hub.bus) = None;
        if let Some(session) = lock(&hub.session).take() {
            session.notify(&Notice::Failure("the server stopped serving the devices"));
            session.close();
        }
        outcome
    }

    fn watch(
        &self,
        watch: &Watch<'_>,
        health: &mut Health,
        on_status: &mut impl FnMut(&Status),
    ) -> Result<(), ServerError> {
        let checker = watch.bus.state_checker();
        let mut next = Instant::now();
        while !(watch.stop.load(Ordering::Acquire) || watch.ended.load(Ordering::Acquire)) {
            let now = Instant::now();
            if now < next {
                std::thread::sleep((next - now).min(POLL_INTERVAL));
                continue;
            }
            next = now + self.option.monitor_interval;
            let devices = watch.monitor.read()?;
            report(health.observe(&devices));
            let session = watch.hub.session();
            let snapshot = checker.check().ok();
            let status = Status::serving(
                session.as_ref().map(|session| session.peer),
                *lock(&watch.hub.relayed),
                BusCounters::new(&watch.bus.stats()),
                session
                    .as_ref()
                    .and(snapshot.as_ref())
                    .map(DeviceStatus::devices),
                health.known(),
            );
            on_status(&status);
            let lost = snapshot.is_some_and(|snapshot| snapshot.any_lost());
            if session.is_none() && (health.any_silent() || lost) {
                tracing::info!(
                    "a device is silent or lost while no client is attached; enumerating again"
                );
                return Ok(());
            }
        }
        Ok(())
    }
}
