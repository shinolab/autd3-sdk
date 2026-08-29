use std::io::{ErrorKind, Read, Write};
use std::marker::PhantomData;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

use autd3_rs_core::link::Link;
use autd3_rs_core::{RX_FRAME_BYTES, TX_FRAME_BYTES};

use crate::bus::{BusOption, BusShared, Desired, SharedBus, run_bus_loop, run_status_loop};
use crate::error::RemoteLinkError;
use crate::wire::{self, BusStatus};
use crate::{DeviceLayout, wire::REPLY_HEADER_BYTES};

const DEFAULT_PORT: u16 = 8080;
const STACK_HEADROOM_BYTES: usize = 1024 * 1024;
pub(crate) const SESSION_OPEN_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(10);
const KEEPALIVE_IDLE: Duration = Duration::from_secs(5);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug)]
pub struct RemoteServerOption {
    pub bind: SocketAddr,
    pub bus: BusOption,
    pub idle_timeout: Duration,
}

impl Default for RemoteServerOption {
    fn default() -> Self {
        Self {
            bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), DEFAULT_PORT),
            bus: BusOption::default(),
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
        }
    }
}

impl RemoteServerOption {
    #[must_use]
    pub fn new(bind: SocketAddr) -> Self {
        Self {
            bind,
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BusServerOption {
    pub bind: SocketAddr,
    pub auto_open: bool,
    pub idle_timeout: Duration,
}

impl Default for BusServerOption {
    fn default() -> Self {
        Self {
            bind: SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), DEFAULT_PORT),
            auto_open: true,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
        }
    }
}

impl BusServerOption {
    #[must_use]
    pub fn new(bind: SocketAddr) -> Self {
        Self {
            bind,
            ..Self::default()
        }
    }
}

pub struct RemoteServer<L, F>
where
    L: Link,
    F: FnMut(&[DeviceLayout]) -> Result<L, RemoteLinkError> + Send,
{
    listener: TcpListener,
    option: RemoteServerOption,
    factory: F,
    _link: PhantomData<fn() -> L>,
}

impl<L, F> RemoteServer<L, F>
where
    L: Link,
    F: FnMut(&[DeviceLayout]) -> Result<L, RemoteLinkError> + Send,
{
    pub fn new(option: RemoteServerOption, factory: F) -> Result<Self, RemoteLinkError> {
        Ok(Self {
            listener: TcpListener::bind(option.bind)?,
            option,
            factory,
            _link: PhantomData,
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, RemoteLinkError> {
        Ok(self.listener.local_addr()?)
    }

    pub fn serve(&mut self) -> Result<(), RemoteLinkError> {
        loop {
            if let Some((stream, peer)) = accept(&self.listener)? {
                report_session(peer, self.handle_client(stream));
            }
        }
    }

    pub fn serve_once(&mut self) -> Result<(), RemoteLinkError> {
        let (stream, _peer) = self.listener.accept()?;
        self.handle_client(stream)
    }

    pub fn serve_with_factory(
        option: RemoteServerOption,
        factory: F,
    ) -> Result<(), RemoteLinkError> {
        Self::new(option, factory)?.serve()
    }

    fn handle_client(&mut self, mut stream: TcpStream) -> Result<(), RemoteLinkError> {
        tune_session_socket(&stream, self.option.idle_timeout)?;
        handshake(&mut stream)?;
        let layout = wire::read_geometry(&mut stream)?;

        let shared = Arc::new(BusShared::new());
        let (checker_tx, checker_rx) = std::sync::mpsc::channel();
        let factory = &mut self.factory;
        let bus_option = &self.option.bus;
        let layout = &layout;
        shared.set_desired(Desired::Open);

        std::thread::scope(|scope| {
            let mut builder = std::thread::Builder::new().name("autd3-remote-bus".to_owned());
            if bus_option.stack_prefault_bytes > 0 {
                builder =
                    builder.stack_size(bus_option.stack_prefault_bytes + STACK_HEADROOM_BYTES);
            }
            let bus_shared = Arc::clone(&shared);
            let bus = builder
                .spawn_scoped(scope, move || {
                    run_bus_loop(&bus_shared, bus_option, || factory(layout), &checker_tx);
                })
                .map_err(|e| RemoteLinkError::Link(format!("failed to spawn bus thread: {e}")))?;
            let status_shared = Arc::clone(&shared);
            let status = std::thread::Builder::new()
                .name("autd3-remote-status".to_owned())
                .spawn_scoped(scope, move || {
                    run_status_loop(&status_shared, bus_option, &checker_rx);
                })
                .map_err(|e| {
                    RemoteLinkError::Link(format!("failed to spawn status thread: {e}"))
                })?;

            let result = serve_session(&mut stream, &shared, bus_option, layout.len(), true);

            shared.stop();
            let _ = bus.join();
            let _ = status.join();
            result
        })
    }
}

#[derive(Clone, Debug)]
pub struct Session {
    pub peer: SocketAddr,
    pub devices: usize,
    pub since: Instant,
}

#[derive(Default)]
pub struct Sessions {
    current: std::sync::Mutex<Option<Session>>,
}

impl Sessions {
    #[must_use]
    pub fn current(&self) -> Option<Session> {
        self.lock().clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Session>> {
        self.current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn open(&self, peer: SocketAddr, devices: usize) {
        *self.lock() = Some(Session {
            peer,
            devices,
            since: Instant::now(),
        });
    }

    fn close(&self) {
        *self.lock() = None;
    }
}

pub struct BusServer {
    listener: TcpListener,
    option: BusServerOption,
    bus: Arc<SharedBus>,
    sessions: Arc<Sessions>,
}

impl BusServer {
    pub fn new(option: BusServerOption, bus: Arc<SharedBus>) -> Result<Self, RemoteLinkError> {
        Ok(Self {
            listener: TcpListener::bind(option.bind)?,
            option,
            bus,
            sessions: Arc::new(Sessions::default()),
        })
    }

    #[must_use]
    pub fn sessions(&self) -> Arc<Sessions> {
        Arc::clone(&self.sessions)
    }

    pub fn local_addr(&self) -> Result<SocketAddr, RemoteLinkError> {
        Ok(self.listener.local_addr()?)
    }

    pub fn serve(&mut self) -> Result<(), RemoteLinkError> {
        loop {
            if let Some((stream, peer)) = accept(&self.listener)? {
                report_session(peer, self.handle_client(stream));
            }
        }
    }

    pub fn serve_once(&mut self) -> Result<(), RemoteLinkError> {
        let (stream, _peer) = self.listener.accept()?;
        self.handle_client(stream)
    }

    fn handle_client(&mut self, mut stream: TcpStream) -> Result<(), RemoteLinkError> {
        tune_session_socket(&stream, self.option.idle_timeout)?;
        let peer = stream.peer_addr()?;
        handshake(&mut stream)?;
        let layout = wire::read_geometry(&mut stream)?;

        self.sessions.open(peer, layout.len());
        let result = serve_session(
            &mut stream,
            self.bus.shared(),
            self.bus.option(),
            layout.len(),
            self.option.auto_open,
        );
        self.sessions.close();
        result
    }
}

fn tune_session_socket(stream: &TcpStream, idle_timeout: Duration) -> std::io::Result<()> {
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(idle_timeout))?;
    stream.set_write_timeout(Some(idle_timeout))?;
    socket2::SockRef::from(stream).set_tcp_keepalive(
        &socket2::TcpKeepalive::new()
            .with_time(KEEPALIVE_IDLE)
            .with_interval(KEEPALIVE_INTERVAL),
    )
}

fn is_timeout(error: &std::io::Error) -> bool {
    matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
}

fn accept(listener: &TcpListener) -> Result<Option<(TcpStream, SocketAddr)>, RemoteLinkError> {
    match listener.accept() {
        Ok(accepted) => Ok(Some(accepted)),
        Err(e)
            if is_timeout(&e)
                || matches!(
                    e.kind(),
                    ErrorKind::ConnectionAborted
                        | ErrorKind::ConnectionReset
                        | ErrorKind::Interrupted
                ) =>
        {
            tracing::warn!(error = %e, "failed to accept a client; still listening");
            Ok(None)
        }
        Err(e) => Err(e.into()),
    }
}

fn report_session(peer: SocketAddr, result: Result<(), RemoteLinkError>) {
    match result {
        Ok(()) => tracing::info!(%peer, "client disconnected"),
        Err(e) => tracing::warn!(%peer, error = %e, "client connection terminated"),
    }
}

fn serve_session(
    stream: &mut TcpStream,
    shared: &Arc<BusShared>,
    bus_option: &BusOption,
    client_devices: usize,
    auto_open: bool,
) -> Result<(), RemoteLinkError> {
    let num_devices = match attach(shared, client_devices, auto_open) {
        Ok(num_devices) => num_devices,
        Err(e) => {
            let (code, detail) = reject_of(&e);
            stream.write_all(&wire::encode_session_reject(code, &detail))?;
            stream.flush()?;
            return Err(e);
        }
    };

    let n = u16::try_from(num_devices)
        .map_err(|_| RemoteLinkError::InvalidDeviceCount { found: num_devices })?;
    stream.write_all(&wire::encode_session_ok(n))?;
    stream.flush()?;

    std::thread::scope(|scope| {
        let session = std::thread::Builder::new()
            .name("autd3-remote-session".to_owned())
            .spawn_scoped(scope, || {
                autd3_rs_core::apply_thread_tuning(bus_option.session_tuning());
                run_frame_loop(stream, shared, num_devices)
            })
            .map_err(|e| RemoteLinkError::Link(format!("failed to spawn session thread: {e}")))?;
        session
            .join()
            .unwrap_or_else(|_| Err(RemoteLinkError::Link("session thread panicked".to_owned())))
    })
}

fn attach(
    shared: &BusShared,
    client_devices: usize,
    auto_open: bool,
) -> Result<usize, RemoteLinkError> {
    if let Some(reason) = shared.hold_reason() {
        return Err(RemoteLinkError::BusUnavailable { reason });
    }
    if shared.desired() == Desired::Closed {
        if !auto_open {
            return Err(RemoteLinkError::BusClosed);
        }
        tracing::info!("opening the bus for an incoming client");
        shared.set_desired(Desired::Open);
    }
    let num_devices = shared.wait_for_open(SESSION_OPEN_TIMEOUT)?;
    if num_devices != client_devices {
        return Err(RemoteLinkError::GeometryMismatch {
            client: client_devices,
            bus: num_devices,
        });
    }
    Ok(num_devices)
}

fn reject_of(error: &RemoteLinkError) -> (u8, String) {
    let code = match error {
        RemoteLinkError::BusClosed => wire::SESSION_BUS_CLOSED,
        RemoteLinkError::GeometryMismatch { .. } => wire::SESSION_DEVICE_COUNT,
        RemoteLinkError::BusUnavailable { .. } => wire::SESSION_BUS_UNAVAILABLE,
        _ => wire::SESSION_INTERNAL,
    };
    (code, error.to_string())
}

fn handshake(stream: &mut TcpStream) -> Result<(), RemoteLinkError> {
    let peer = wire::read_hello(stream)?;
    stream.write_all(&wire::encode_hello())?;
    stream.flush()?;
    if peer.as_ref().is_none_or(|p| p.wire != wire::VERSION) {
        return Err(RemoteLinkError::ProtocolMismatch {
            local: wire::local_version(),
            peer,
        });
    }
    Ok(())
}

fn run_frame_loop(
    stream: &mut TcpStream,
    shared: &BusShared,
    num_devices: usize,
) -> Result<(), RemoteLinkError> {
    let mut tx_buf = vec![0u8; num_devices * TX_FRAME_BYTES];
    let mut rx_buf = vec![0u8; num_devices * RX_FRAME_BYTES];
    let mut status = BusStatus::new(num_devices);
    let mut reply = Vec::with_capacity(REPLY_HEADER_BYTES + num_devices);

    loop {
        let mut tag = [0u8; 1];
        match stream.read_exact(&mut tag) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) if is_timeout(&e) => {
                return Err(RemoteLinkError::Link(
                    "the client stopped sending frames; ending the session".to_owned(),
                ));
            }
            Err(e) => return Err(e.into()),
        }

        match tag[0] {
            wire::TAG_FRAME => {
                stream.read_exact(&mut tx_buf)?;
                let frame = shared.exchange(num_devices, &tx_buf, &mut rx_buf, &mut status)?;
                wire::encode_reply_header(frame.rx_valid, frame.dc_time_ns, &status, &mut reply);
                reply.extend_from_slice(&rx_buf);
                stream.write_all(&reply)?;
                stream.flush()?;
            }
            wire::TAG_CLOSE => return Ok(()),
            other => return Err(RemoteLinkError::UnexpectedTag(other)),
        }
    }
}
