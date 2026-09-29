use std::io;
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::{
    AssignIdBody, FLAG_ASSIGNED, FLAG_DOWNSTREAM_LINK, FLAG_DOWNSTREAM_OPEN, FLAG_SYNC_READY,
    FrameReply, Header, Kind, PROTOCOL_VERSION, RESET_ID_CLOSE_DELAY_MS, SetTimeBody, Status,
    UNASSIGNED_ID, UPSTREAM_UNKNOWN, UnblockReply, UnitInfo,
};
use autd3_rs_core::TX_FRAME_BYTES;
use autd3_rs_core::value::DcSysTime;
use autd3_rs_firmware_emulator::Device;
use zerocopy::little_endian::U64;
use zerocopy::{FromBytes, IntoBytes};

use super::option::TransportOption;

const NUM_TRANSDUCERS: usize = 249;
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const PULSE_DELAY: Duration = Duration::from_millis(50);
const MAX_TICKS_PER_FRAME: u128 = 1000;
const UPSTREAM_PORT: u8 = 0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Via {
    Group,
    Unit(usize),
}

struct Unit {
    device: Device,
    socket: Arc<UdpSocket>,
    id: Option<u8>,
    clock_offset_ns: i64,
    open: bool,
    close_at: Option<Instant>,
    ready_at: Option<Instant>,
    last_tick: Instant,
}

impl Unit {
    fn forget(&mut self) {
        self.id = None;
        self.open = false;
        self.close_at = None;
        self.ready_at = None;
    }
}

struct Chain {
    units: Vec<Unit>,
}

fn host_now_ns() -> i64 {
    DcSysTime::now().map_or(0, |t| i64::try_from(t.sys_time()).unwrap_or(i64::MAX))
}

impl Chain {
    fn unit_now(&self, index: usize) -> u64 {
        u64::try_from(host_now_ns().saturating_add(self.units[index].clock_offset_ns)).unwrap_or(0)
    }

    fn apply_pending(&mut self, now: Instant) {
        for unit in &mut self.units {
            if unit.close_at.is_some_and(|at| at <= now) {
                unit.forget();
            }
        }
    }

    fn reachable(&self, index: usize) -> bool {
        self.units[..index].iter().all(|u| u.open)
    }

    fn targets(&self, via: Via) -> Vec<usize> {
        match via {
            Via::Group => (0..self.units.len())
                .take_while(|&i| self.reachable(i))
                .collect(),
            Via::Unit(i) if self.reachable(i) => vec![i],
            Via::Unit(_) => Vec::new(),
        }
    }

    fn flags(&self, index: usize, now: Instant) -> u8 {
        let unit = &self.units[index];
        let mut flags = 0;
        if unit.id.is_some() {
            flags |= FLAG_ASSIGNED;
        }
        if unit.open {
            flags |= FLAG_DOWNSTREAM_OPEN;
        }
        if index + 1 < self.units.len() {
            flags |= FLAG_DOWNSTREAM_LINK;
        }
        if unit.ready_at.is_some_and(|at| at <= now) {
            flags |= FLAG_SYNC_READY;
        }
        flags
    }

    fn reply(&self, index: usize, dst: SocketAddr, header: Header, body: &[u8]) {
        let mut datagram = header.as_bytes().to_vec();
        datagram.extend_from_slice(body);
        let _ = self.units[index].socket.send_to(&datagram, dst);
    }

    fn reply_status(
        &self,
        index: usize,
        dst: SocketAddr,
        header: Header,
        status: Status,
        extra: &[u8],
    ) {
        let mut body = vec![status.as_u8()];
        body.extend_from_slice(extra);
        self.reply(index, dst, header, &body);
    }

    fn unit_info(&self, index: usize, now: Instant) -> UnitInfo {
        let unit = &self.units[index];
        UnitInfo {
            unit_id: unit.id.unwrap_or(UNASSIGNED_ID),
            flags: self.flags(index, now),
            upstream_port: if unit.id.is_some() {
                UPSTREAM_PORT
            } else {
                UPSTREAM_UNKNOWN
            },
            reserved: 0,
            fw_version: [
                autd3_rs_firmware_emulator::autd3_cpu_fw::FW_VERSION_MAJOR,
                autd3_rs_firmware_emulator::autd3_cpu_fw::FW_VERSION_MINOR,
                autd3_rs_firmware_emulator::autd3_cpu_fw::FW_VERSION_PATCH,
            ],
            reserved2: 0,
            sys_time: U64::new(self.unit_now(index)),
        }
    }

    fn handle(&mut self, via: Via, src: SocketAddr, datagram: &[u8]) {
        let now = Instant::now();
        self.apply_pending(now);
        let Ok((header, body)) = Header::read_from_prefix(datagram) else {
            return;
        };
        let targets = self.targets(via);
        if header.version != PROTOCOL_VERSION {
            let reply = Header {
                version: PROTOCOL_VERSION,
                ..header
            };
            for &i in &targets {
                self.reply_status(i, src, reply, Status::UnsupportedVersion, &[]);
            }
            return;
        }
        let Some(kind) = Kind::from_u8(header.kind) else {
            return;
        };
        let unicast = matches!(via, Via::Unit(_));
        match kind {
            Kind::Frame if unicast => {
                for i in targets {
                    self.frame(i, src, header, body, now);
                }
            }
            Kind::ReadUnitInfo => {
                for i in targets {
                    let info = self.unit_info(i, now);
                    self.reply_status(i, src, header, Status::Ok, info.as_bytes());
                }
            }
            Kind::ResetId => {
                for i in targets {
                    self.reply_status(i, src, header, Status::Ok, &[]);
                    self.units[i].close_at.get_or_insert(
                        now + Duration::from_millis(u64::from(RESET_ID_CLOSE_DELAY_MS)),
                    );
                }
            }
            Kind::Discover => {
                for i in targets {
                    if self.units[i].id.is_none() {
                        self.reply_status(i, src, header, Status::Ok, &[]);
                    }
                }
            }
            Kind::AssignId if unicast => {
                for i in targets {
                    self.assign(i, src, header, body);
                }
            }
            Kind::UnblockDownstream if unicast => {
                for i in targets {
                    if self.units[i].id.is_none() {
                        self.reply_status(i, src, header, Status::NotAssigned, &[]);
                        continue;
                    }
                    self.units[i].open = true;
                    let reply = UnblockReply {
                        downstream_link: u8::from(i + 1 < self.units.len()),
                    };
                    self.reply_status(i, src, header, Status::Ok, reply.as_bytes());
                }
            }
            Kind::SetTime if unicast => {
                for i in targets {
                    self.set_time(i, src, header, body, now);
                }
            }
            _ => {}
        }
    }

    fn assign(&mut self, index: usize, src: SocketAddr, header: Header, body: &[u8]) {
        if self.units[index].id.is_some() {
            return;
        }
        let Ok((request, _)) = AssignIdBody::read_from_prefix(body) else {
            self.reply_status(index, src, header, Status::InvalidPayload, &[]);
            return;
        };
        if request.unit_id == UNASSIGNED_ID {
            self.reply_status(index, src, header, Status::InvalidPayload, &[]);
            return;
        }
        self.units[index].id = Some(request.unit_id);
        self.reply_status(index, src, header, Status::Ok, &[]);
    }

    fn set_time(
        &mut self,
        index: usize,
        src: SocketAddr,
        header: Header,
        body: &[u8],
        now: Instant,
    ) {
        let status = if self.units[index].id.is_none() {
            Status::NotAssigned
        } else if let Ok((request, _)) = SetTimeBody::read_from_prefix(body) {
            let target = i64::try_from(request.sys_time.get()).unwrap_or(i64::MAX);
            let unit = &mut self.units[index];
            unit.clock_offset_ns = target.saturating_sub(host_now_ns());
            unit.ready_at = Some(now + PULSE_DELAY);
            Status::Ok
        } else {
            Status::InvalidPayload
        };
        self.reply_status(index, src, header, status, &[]);
    }

    fn frame(&mut self, index: usize, src: SocketAddr, header: Header, body: &[u8], now: Instant) {
        let Some(unit_id) = self.units[index].id else {
            return;
        };
        let Some(tx) = body
            .get(..TX_FRAME_BYTES)
            .and_then(|b| <&[u8; TX_FRAME_BYTES]>::try_from(b).ok())
        else {
            return;
        };
        let bus = self.unit_now(index);
        let flags = self.flags(index, now);
        let unit = &mut self.units[index];
        let elapsed = now.saturating_duration_since(unit.last_tick).as_millis();
        if elapsed > 0 {
            unit.last_tick = now;
            for _ in 0..elapsed.min(MAX_TICKS_PER_FRAME) {
                unit.device.tick_1ms();
            }
        }
        unit.device.fpga_mut().update_with_sys_time(bus);
        unit.device.recv(tx);
        let rx = unit.device.rx();
        let reply = FrameReply {
            ack: rx.ack.get(),
            data: rx.data,
            flags,
            unit_id,
            sys_time: U64::new(bus),
        };
        self.reply(index, src, header, reply.as_bytes());
        self.units[index].device.process_pending();
    }
}

pub struct UdpEmulator {
    chain: Arc<Mutex<Chain>>,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    group: SocketAddrV6,
}

fn bind_loopback() -> io::Result<(Arc<UdpSocket>, SocketAddrV6)> {
    bind(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0))
}

fn bind(addr: SocketAddrV6) -> io::Result<(Arc<UdpSocket>, SocketAddrV6)> {
    let socket = UdpSocket::bind(addr)?;
    socket.set_read_timeout(Some(POLL_INTERVAL))?;
    let SocketAddr::V6(addr) = socket.local_addr()? else {
        return Err(io::Error::from(io::ErrorKind::AddrNotAvailable));
    };
    Ok((Arc::new(socket), addr))
}

fn lock(chain: &Mutex<Chain>) -> MutexGuard<'_, Chain> {
    chain.lock().unwrap_or_else(PoisonError::into_inner)
}

fn serve(chain: &Mutex<Chain>, socket: &UdpSocket, via: Via, stop: &AtomicBool) {
    let mut buf = vec![0u8; 2048];
    while !stop.load(Ordering::Acquire) {
        match socket.recv_from(&mut buf) {
            Ok((len, src)) => lock(chain).handle(via, src, &buf[..len]),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(e) => tracing::debug!("emulated device socket error: {e}"),
        }
    }
}

impl UdpEmulator {
    pub fn spawn(num_devices: usize) -> io::Result<Self> {
        Self::spawn_at(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0), num_devices)
    }

    pub fn spawn_at(group: SocketAddrV6, num_devices: usize) -> io::Result<Self> {
        if !group.ip().is_loopback() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "the emulated devices live on ::1, so the group must be a loopback address, not {group}"
                ),
            ));
        }
        let (group_socket, group) = bind(group)?;
        let now = Instant::now();
        let boot = host_now_ns();
        let mut units = Vec::with_capacity(num_devices);
        for _ in 0..num_devices {
            let (socket, _) = bind_loopback()?;
            units.push(Unit {
                device: Device::new(NUM_TRANSDUCERS),
                socket,
                id: None,
                clock_offset_ns: -boot,
                open: false,
                close_at: None,
                ready_at: None,
                last_tick: now,
            });
        }
        let sockets: Vec<Arc<UdpSocket>> = units.iter().map(|u| Arc::clone(&u.socket)).collect();
        let chain = Arc::new(Mutex::new(Chain { units }));
        let stop = Arc::new(AtomicBool::new(false));
        let mut threads = Vec::with_capacity(num_devices + 1);
        let endpoints = std::iter::once((group_socket, Via::Group)).chain(
            sockets
                .into_iter()
                .enumerate()
                .map(|(i, s)| (s, Via::Unit(i))),
        );
        for (socket, via) in endpoints {
            let chain = Arc::clone(&chain);
            let serving = Arc::clone(&stop);
            let spawned = std::thread::Builder::new()
                .name("udp-emulator".into())
                .spawn(move || serve(&chain, &socket, via, &serving));
            match spawned {
                Ok(thread) => threads.push(thread),
                Err(e) => {
                    stop.store(true, Ordering::Release);
                    for thread in threads {
                        let _ = thread.join();
                    }
                    return Err(e);
                }
            }
        }
        Ok(Self {
            chain,
            stop,
            threads,
            group,
        })
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        lock(&self.chain).units.len()
    }

    #[must_use]
    pub fn device_addr(&self, index: usize) -> SocketAddrV6 {
        match lock(&self.chain).units[index].socket.local_addr() {
            Ok(SocketAddr::V6(addr)) => addr,
            _ => unreachable!("emulated devices bind IPv6 loopback"),
        }
    }

    #[must_use]
    pub fn group(&self) -> SocketAddrV6 {
        self.group
    }

    #[must_use]
    pub fn option(&self) -> TransportOption {
        TransportOption {
            group: Some(self.group),
            reply_timeout: Duration::from_millis(50),
            response_timeout: Duration::from_millis(50),
            enumeration_timeout: Duration::from_secs(1),
            sync_timeout: Duration::from_secs(5),
            ..TransportOption::default()
        }
    }

    pub fn with_device<R>(&self, index: usize, f: impl FnOnce(&mut Device) -> R) -> R {
        f(&mut lock(&self.chain).units[index].device)
    }

    pub fn with_devices<R>(&self, f: impl FnOnce(&[&Device]) -> R) -> R {
        let chain = lock(&self.chain);
        let devices: Vec<&Device> = chain.units.iter().map(|unit| &unit.device).collect();
        f(&devices)
    }

    pub fn reboot(&self, index: usize) {
        let mut chain = lock(&self.chain);
        let unit = &mut chain.units[index];
        unit.device.reset_cpu();
        unit.forget();
    }
}

impl Drop for UdpEmulator {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}
