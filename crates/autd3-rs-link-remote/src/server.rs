use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use autd3_rs_core::link::Link;
use autd3_rs_core::{IntoLink, RX_FRAME_BYTES, TX_FRAME_BYTES};

use crate::error::RemoteLinkError;
use crate::{DeviceLayout, wire};

type GeometryHandler = Arc<dyn Fn(Vec<DeviceLayout>) + Send + Sync>;

const DEFAULT_CYCLE_PERIOD: Duration = Duration::from_micros(250);

struct BusState {
    tx: Vec<[u8; TX_FRAME_BYTES]>,
    rx: Vec<[u8; RX_FRAME_BYTES]>,
    rx_valid: bool,
    tx_version: u64,
    applied_version: u64,
    shutdown: bool,
}

struct Shared {
    state: Mutex<BusState>,
    cv: Condvar,
}

impl Shared {
    fn new(num_devices: usize) -> Self {
        Self {
            state: Mutex::new(BusState {
                tx: vec![[0u8; TX_FRAME_BYTES]; num_devices],
                rx: vec![[0u8; RX_FRAME_BYTES]; num_devices],
                rx_valid: false,
                tx_version: 0,
                applied_version: 0,
                shutdown: false,
            }),
            cv: Condvar::new(),
        }
    }

    fn shutdown(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.shutdown = true;
        self.cv.notify_all();
    }

    fn exchange(&self, tx: &[u8], rx: &mut [u8]) -> Result<bool, RemoteLinkError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.tx.as_flattened_mut().copy_from_slice(tx);
        state.tx_version += 1;
        let want = state.tx_version;
        loop {
            if state.shutdown {
                return Err(RemoteLinkError::Link("bus loop stopped".to_owned()));
            }
            if state.applied_version >= want {
                rx.copy_from_slice(state.rx.as_flattened());
                return Ok(state.rx_valid);
            }
            state = self
                .cv
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
}

fn run_bus_loop<L: Link>(mut link: L, shared: &Shared, cycle_period: Duration) {
    let num_devices = link.num_devices();
    let mut tx_local = vec![[0u8; TX_FRAME_BYTES]; num_devices];
    let mut rx_local = vec![[0u8; RX_FRAME_BYTES]; num_devices];

    loop {
        let version = {
            let state = shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.shutdown {
                break;
            }
            tx_local.copy_from_slice(&state.tx);
            state.tx_version
        };

        let start = Instant::now();
        let result = link.cycle(&tx_local, &mut rx_local);

        {
            let mut state = shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match result {
                Ok(outcome) => {
                    state.rx.copy_from_slice(&rx_local);
                    state.rx_valid = outcome.rx_valid;
                }
                Err(e) => {
                    tracing::error!(error = %e, "bus cycle failed; stopping server bus loop");
                    state.rx_valid = false;
                    state.shutdown = true;
                }
            }
            state.applied_version = version;
            let stop = state.shutdown;
            shared.cv.notify_all();
            if stop {
                break;
            }
        }

        let remaining = cycle_period.saturating_sub(start.elapsed());
        if !remaining.is_zero() {
            std::thread::sleep(remaining);
        }
    }
}

pub struct RemoteServer {
    listener: TcpListener,
    shared: Arc<Shared>,
    num_devices: usize,
    on_geometry: Option<GeometryHandler>,
    rt: Option<JoinHandle<()>>,
}

impl RemoteServer {
    pub fn with_link<L: Link>(bind: SocketAddr, link: L) -> Result<Self, RemoteLinkError> {
        Self::with_link_inner(bind, link, None)
    }

    pub fn with_link_and_geometry<L, F>(
        bind: SocketAddr,
        link: L,
        on_geometry: F,
    ) -> Result<Self, RemoteLinkError>
    where
        L: Link,
        F: Fn(Vec<DeviceLayout>) + Send + Sync + 'static,
    {
        Self::with_link_inner(bind, link, Some(Arc::new(on_geometry)))
    }

    fn with_link_inner<L: Link>(
        bind: SocketAddr,
        link: L,
        on_geometry: Option<GeometryHandler>,
    ) -> Result<Self, RemoteLinkError> {
        let num_devices = link.num_devices();
        if num_devices == 0 {
            return Err(RemoteLinkError::InvalidDeviceCount { found: num_devices });
        }
        let listener = TcpListener::bind(bind)?;
        let shared = Arc::new(Shared::new(num_devices));
        let rt = {
            let shared = Arc::clone(&shared);
            std::thread::Builder::new()
                .name("autd3-remote-bus".to_owned())
                .spawn(move || run_bus_loop(link, &shared, DEFAULT_CYCLE_PERIOD))
                .map_err(|e| RemoteLinkError::Link(format!("failed to spawn bus thread: {e}")))?
        };
        Ok(Self {
            listener,
            shared,
            num_devices,
            on_geometry,
            rt: Some(rt),
        })
    }

    pub async fn open<T: IntoLink>(bind: SocketAddr, link: T) -> Result<Self, RemoteLinkError> {
        let geometry = autd3_rs_core::Geometry::new(Vec::<autd3_rs_core::Device>::new());
        let link = link
            .into_link(&geometry)
            .await
            .map_err(|e| RemoteLinkError::Link(e.to_string()))?;
        Self::with_link(bind, link)
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.num_devices
    }

    pub fn local_addr(&self) -> Result<SocketAddr, RemoteLinkError> {
        Ok(self.listener.local_addr()?)
    }

    pub fn serve(&mut self) -> Result<(), RemoteLinkError> {
        loop {
            let (stream, peer) = self.listener.accept()?;
            tracing::info!(%peer, "client connected");
            match self.handle_client(stream) {
                Ok(()) => tracing::info!(%peer, "client disconnected"),
                Err(e) => tracing::warn!(%peer, error = %e, "client connection terminated"),
            }
        }
    }

    pub fn serve_once(&mut self) -> Result<(), RemoteLinkError> {
        let (stream, _peer) = self.listener.accept()?;
        self.handle_client(stream)
    }

    pub fn serve_with_factory<L, F>(bind: SocketAddr, mut factory: F) -> Result<(), RemoteLinkError>
    where
        L: Link,
        F: FnMut(&[DeviceLayout]) -> Result<L, RemoteLinkError>,
    {
        let listener = TcpListener::bind(bind)?;
        loop {
            let (stream, peer) = listener.accept()?;
            tracing::info!(%peer, "client connected");
            match handle_factory_client(stream, &mut factory) {
                Ok(()) => tracing::info!(%peer, "client disconnected"),
                Err(e) => tracing::warn!(%peer, error = %e, "client connection terminated"),
            }
        }
    }

    fn handle_client(&mut self, mut stream: TcpStream) -> Result<(), RemoteLinkError> {
        stream.set_nodelay(true)?;
        read_hello(&mut stream)?;
        let layout = wire::read_geometry(&mut stream)?;
        if let Some(cb) = &self.on_geometry {
            cb(layout);
        }
        send_num_devices(&mut stream, self.num_devices)?;
        run_frame_loop(&mut stream, &self.shared, self.num_devices)
    }
}

fn handle_factory_client<L, F>(
    mut stream: TcpStream,
    factory: &mut F,
) -> Result<(), RemoteLinkError>
where
    L: Link,
    F: FnMut(&[DeviceLayout]) -> Result<L, RemoteLinkError>,
{
    stream.set_nodelay(true)?;
    read_hello(&mut stream)?;
    let layout = wire::read_geometry(&mut stream)?;

    let link = factory(&layout)?;
    let num_devices = link.num_devices();
    if num_devices == 0 {
        return Err(RemoteLinkError::InvalidDeviceCount { found: num_devices });
    }

    let shared = Arc::new(Shared::new(num_devices));
    let rt = {
        let shared = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("autd3-remote-bus".to_owned())
            .spawn(move || run_bus_loop(link, &shared, DEFAULT_CYCLE_PERIOD))
            .map_err(|e| RemoteLinkError::Link(format!("failed to spawn bus thread: {e}")))?
    };

    let result = send_num_devices(&mut stream, num_devices)
        .and_then(|()| run_frame_loop(&mut stream, &shared, num_devices));

    shared.shutdown();
    let _ = rt.join();
    result
}

fn read_hello(stream: &mut TcpStream) -> Result<(), RemoteLinkError> {
    let mut hello = [0u8; 5];
    stream.read_exact(&mut hello)?;
    if hello[..4] != wire::MAGIC || hello[4] != wire::VERSION {
        return Err(RemoteLinkError::ProtocolMismatch);
    }
    Ok(())
}

fn send_num_devices(stream: &mut TcpStream, num_devices: usize) -> Result<(), RemoteLinkError> {
    let n = u16::try_from(num_devices)
        .map_err(|_| RemoteLinkError::InvalidDeviceCount { found: num_devices })?;
    stream.write_all(&n.to_le_bytes())?;
    stream.flush()?;
    Ok(())
}

fn run_frame_loop(
    stream: &mut TcpStream,
    shared: &Shared,
    num_devices: usize,
) -> Result<(), RemoteLinkError> {
    let mut tx_buf = vec![0u8; num_devices * TX_FRAME_BYTES];
    let mut rx_buf = vec![0u8; num_devices * RX_FRAME_BYTES];

    loop {
        let mut tag = [0u8; 1];
        match stream.read_exact(&mut tag) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e.into()),
        }

        match tag[0] {
            wire::TAG_FRAME => {
                stream.read_exact(&mut tx_buf)?;
                let rx_valid = shared.exchange(&tx_buf, &mut rx_buf)?;
                stream.write_all(&[u8::from(rx_valid)])?;
                stream.write_all(&rx_buf)?;
                stream.flush()?;
            }
            wire::TAG_CLOSE => return Ok(()),
            other => return Err(RemoteLinkError::UnexpectedTag(other)),
        }
    }
}

impl Drop for RemoteServer {
    fn drop(&mut self) {
        {
            let mut state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.shutdown = true;
        }
        self.shared.cv.notify_all();
        if let Some(rt) = self.rt.take() {
            let _ = rt.join();
        }
    }
}
