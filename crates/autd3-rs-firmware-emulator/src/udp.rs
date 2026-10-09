use std::io;
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use autd3_cpu_fw::proto::{Disposition, Drained, Reply};
use autd3_cpu_fw::ptp::Config as PtpConfig;
use autd3_cpu_wire::udp::{
    AssignIdBody, Flags, FrameReply, Header, Kind, PORT, PROTOCOL_VERSION, RESET_ID_CLOSE_DELAY_MS,
    Role, SetTimeBody, Status, UNASSIGNED_ID, UPSTREAM_UNKNOWN, UnblockReply, UnitInfo,
};
use autd3_rs_core::geometry::Autd3;
use autd3_rs_core::{FRAME_BYTES_MAX, Interface};
use zerocopy::little_endian::{I32, U64};
use zerocopy::{FromBytes, IntoBytes};

use crate::Device;

const SIMULATOR: SocketAddrV6 = SocketAddrV6::new(Ipv6Addr::LOCALHOST, PORT, 0, 0);
const LOOPBACK_ANY: SocketAddrV6 = SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const PULSE_DELAY: Duration = Duration::from_millis(50);
const LOCK_DELAY: Duration = Duration::from_millis(50);
const MAX_CATCH_UP_TICKS: u128 = 60_000;
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
    grandmaster: bool,
    clock_offset_ns: i64,
    open: bool,
    close_at: Option<Instant>,
    locked_at: Option<Instant>,
    lock_blocked: bool,
    ptp_config: PtpConfig,
    last_tick: Instant,
    last_host: Option<Instant>,
    drop_frames: u32,
    drop_replies: u32,
    duplicate_frames: u32,
    muted: bool,
    heartbeats: u64,
}

impl Unit {
    fn forget(&mut self) {
        self.id = None;
        self.grandmaster = false;
        self.open = false;
        self.close_at = None;
        self.device.fpga_mut().take_ptp_config();
        self.ptp_config = PtpConfig::default();
        self.unlock();
    }

    fn forget_host(&mut self) {
        self.last_host = None;
        self.device.fpga_mut().host_idle_ms = None;
    }

    fn host_idle_ms(&self, now: Instant) -> Option<u32> {
        self.last_host.map(|at| {
            u32::try_from(now.saturating_duration_since(at).as_millis()).unwrap_or(u32::MAX)
        })
    }

    fn can_lock(&self) -> bool {
        !self.lock_blocked && !self.ptp_config.lock_threshold.is_zero()
    }

    fn unlock(&mut self) {
        self.locked_at = None;
    }

    fn lock_at(&mut self, at: Instant) {
        self.locked_at = Some(at);
    }
}

struct Chain {
    units: Vec<Unit>,
    boot: Instant,
}

impl Chain {
    fn host_now_ns(&self) -> i64 {
        i64::try_from(self.boot.elapsed().as_nanos()).unwrap_or(i64::MAX)
    }

    fn unit_now(&self, index: usize) -> u64 {
        u64::try_from(
            self.host_now_ns()
                .saturating_add(self.units[index].clock_offset_ns),
        )
        .unwrap_or(0)
    }

    fn grandmaster_offset(&self) -> Option<i64> {
        self.units
            .iter()
            .find(|u| u.id.is_some() && u.grandmaster && u.locked_at.is_some())
            .map(|u| u.clock_offset_ns)
    }

    fn follow_grandmaster(&mut self, index: usize, now: Instant) {
        let Some(offset) = self.grandmaster_offset() else {
            return;
        };
        let unit = &mut self.units[index];
        if unit.id.is_none() || unit.grandmaster || !unit.can_lock() {
            return;
        }
        unit.clock_offset_ns = offset;
        unit.lock_at(now + LOCK_DELAY);
    }

    fn apply_ptp_config(&mut self, index: usize, now: Instant) {
        let unit = &mut self.units[index];
        let Some(config) = unit.device.fpga_mut().take_ptp_config() else {
            return;
        };
        unit.ptp_config = config;
        if unit.locked_at.is_none() {
            self.follow_grandmaster(index, now);
        }
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

    fn flags(&self, index: usize, now: Instant) -> Flags {
        let unit = &self.units[index];
        let mut flags = Flags::empty();
        if unit.id.is_some() {
            flags |= Flags::ASSIGNED;
        }
        if unit.open {
            flags |= Flags::DOWNSTREAM_OPEN;
        }
        if index + 1 < self.units.len() {
            flags |= Flags::DOWNSTREAM_LINK;
        }
        if unit.locked_at.is_some_and(|at| at + PULSE_DELAY <= now) {
            flags |= Flags::SYNC_READY;
        }
        if unit.locked_at.is_some_and(|at| at <= now) {
            flags |= Flags::PTP_LOCKED;
        }
        if unit.grandmaster {
            flags |= Flags::GRANDMASTER;
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
                autd3_cpu_fw::FW_VERSION_MAJOR,
                autd3_cpu_fw::FW_VERSION_MINOR,
                autd3_cpu_fw::FW_VERSION_PATCH,
            ],
            reserved2: 0,
            sys_time: U64::new(self.unit_now(index)),
            ptp_offset_ns: I32::new(0),
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
        let unicast_only = matches!(
            kind,
            Kind::Frame
                | Kind::Heartbeat
                | Kind::AssignId
                | Kind::UnblockDownstream
                | Kind::SetTime
        );
        if unicast_only && via == Via::Group {
            return;
        }
        for i in targets {
            match kind {
                Kind::Frame => self.frame(i, src, header, body, now),
                Kind::Heartbeat => self.heartbeat(i, src, header, now),
                Kind::ReadUnitInfo => {
                    let info = self.unit_info(i, now);
                    self.reply_status(i, src, header, Status::Ok, info.as_bytes());
                }
                Kind::ResetId => {
                    self.reply_status(i, src, header, Status::Ok, &[]);
                    self.units[i].close_at.get_or_insert(
                        now + Duration::from_millis(u64::from(RESET_ID_CLOSE_DELAY_MS)),
                    );
                }
                Kind::Discover => {
                    if self.units[i].id.is_none() {
                        self.reply_status(i, src, header, Status::Ok, &[]);
                    }
                }
                Kind::AssignId => self.assign(i, src, header, body, now),
                Kind::UnblockDownstream => {
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
                Kind::SetTime => self.set_time(i, src, header, body, now),
                _ => {}
            }
        }
    }

    fn assign(&mut self, index: usize, src: SocketAddr, header: Header, body: &[u8], now: Instant) {
        if self.units[index].id.is_some() {
            return;
        }
        let Ok((request, _)) = AssignIdBody::read_from_prefix(body) else {
            self.reply_status(index, src, header, Status::InvalidPayload, &[]);
            return;
        };
        let Some(role) = Role::from_u8(request.role) else {
            self.reply_status(index, src, header, Status::InvalidPayload, &[]);
            return;
        };
        if request.unit_id == UNASSIGNED_ID {
            self.reply_status(index, src, header, Status::InvalidPayload, &[]);
            return;
        }
        let unit = &mut self.units[index];
        unit.id = Some(request.unit_id);
        unit.grandmaster = role == Role::Grandmaster;
        self.follow_grandmaster(index, now);
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
        } else if !self.units[index].grandmaster {
            Status::NotGrandmaster
        } else if let Ok((request, _)) = SetTimeBody::read_from_prefix(body) {
            let target = i64::try_from(request.sys_time.get()).unwrap_or(i64::MAX);
            let host = self.host_now_ns();
            let unit = &mut self.units[index];
            unit.clock_offset_ns = target.saturating_sub(host);
            unit.lock_at(now);
            for slave in 0..self.units.len() {
                self.follow_grandmaster(slave, now);
            }
            Status::Ok
        } else {
            Status::InvalidPayload
        };
        self.reply_status(index, src, header, status, &[]);
    }

    fn advance(&mut self, index: usize, now: Instant) {
        let bus = self.unit_now(index);
        let unit = &mut self.units[index];
        let resets = unit.device.fpga().reset_count();
        unit.device.fpga_mut().host_idle_ms = unit.host_idle_ms(now);
        let elapsed = now.saturating_duration_since(unit.last_tick).as_millis();
        if elapsed > 0 {
            unit.last_tick = now;
            for _ in 0..elapsed.min(MAX_CATCH_UP_TICKS) {
                let before = unit.device.fpga().reset_count();
                unit.device.tick_1ms();
                if unit.device.fpga().reset_count() != before {
                    unit.forget_host();
                }
            }
        }
        if unit.device.fpga().reset_count() != resets {
            unit.forget();
        }
        unit.device.fpga_mut().update_with_sys_time(bus);
    }

    fn idle(&mut self, index: usize, now: Instant) {
        self.apply_pending(now);
        self.advance(index, now);
    }

    fn frame_reply(&self, index: usize, dst: SocketAddr, header: Header, now: Instant) {
        let Some(unit_id) = self.units[index].id else {
            return;
        };
        let state: Reply = self.units[index].device.reply();
        let head = FrameReply {
            ack: state.ack,
            status: state.status.as_u8(),
            flags: self.flags(index, now),
            unit_id,
            sys_time: U64::new(self.unit_now(index)),
        };
        let mut body = head.as_bytes().to_vec();
        body.extend_from_slice(state.data());
        self.reply(index, dst, header, &body);
    }

    fn heartbeat(&mut self, index: usize, src: SocketAddr, header: Header, now: Instant) {
        if self.units[index].id.is_none() {
            return;
        }
        self.units[index].heartbeats += 1;
        if self.units[index].muted {
            self.advance(index, now);
            return;
        }
        self.units[index].last_host = Some(now);
        self.advance(index, now);
        if self.units[index].drop_replies > 0 {
            self.units[index].drop_replies -= 1;
            return;
        }
        self.frame_reply(index, src, header, now);
    }

    fn frame(&mut self, index: usize, src: SocketAddr, header: Header, body: &[u8], now: Instant) {
        if self.units[index].id.is_none() || !(2..=FRAME_BYTES_MAX).contains(&body.len()) {
            return;
        }
        if self.units[index].drop_frames > 0 {
            self.units[index].drop_frames -= 1;
            return;
        }
        if self.units[index].muted {
            self.advance(index, now);
            return;
        }
        self.units[index].last_host = Some(now);
        self.advance(index, now);
        if self.units[index].id.is_none() {
            return;
        }
        let disposition = self.units[index].device.recv(body, header.msg_id.get());
        let silent = self.units[index].drop_replies > 0;
        if silent {
            self.units[index].drop_replies -= 1;
        }
        if self.units[index].duplicate_frames > 0 {
            self.units[index].duplicate_frames -= 1;
            if self.units[index].device.recv(body, header.msg_id.get()) == Disposition::Reply
                && !silent
            {
                self.frame_reply(index, src, header, now);
            }
        }
        match disposition {
            Disposition::Reply if silent => {}
            Disposition::Reply => self.frame_reply(index, src, header, now),
            Disposition::Deferred => loop {
                let drained = self.units[index].device.process_one();
                self.apply_ptp_config(index, now);
                match drained {
                    Drained::Empty => break,
                    Drained::Completed { .. } if silent => {}
                    Drained::Completed { msg_id } => {
                        let header = Header {
                            msg_id: zerocopy::little_endian::U16::new(msg_id),
                            ..header
                        };
                        self.frame_reply(index, src, header, now);
                    }
                }
            },
            Disposition::Dropped => {}
        }
    }
}

pub struct UdpEmulator {
    chain: Arc<Mutex<Chain>>,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    group: SocketAddrV6,
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
                ) =>
            {
                if let Via::Unit(index) = via {
                    lock(chain).idle(index, Instant::now());
                }
            }
            Err(e) => tracing::debug!("emulated device socket error: {e}"),
        }
    }
}

impl UdpEmulator {
    pub fn spawn(num_devices: usize) -> io::Result<Self> {
        Self::spawn_at(LOOPBACK_ANY, num_devices)
    }

    pub fn spawn_simulator(num_devices: usize) -> io::Result<Self> {
        Self::spawn_at(SIMULATOR, num_devices)
    }

    fn spawn_at(group: SocketAddrV6, num_devices: usize) -> io::Result<Self> {
        let (group_socket, group) = bind(group)?;
        let now = Instant::now();
        let mut units = Vec::with_capacity(num_devices);
        for _ in 0..num_devices {
            let (socket, _) = bind(LOOPBACK_ANY)?;
            units.push(Unit {
                device: Device::new(Autd3::NUM_TRANSDUCERS),
                socket,
                id: None,
                grandmaster: false,
                clock_offset_ns: 0,
                open: false,
                close_at: None,
                locked_at: None,
                lock_blocked: false,
                ptp_config: PtpConfig::default(),
                last_tick: now,
                last_host: None,
                drop_frames: 0,
                drop_replies: 0,
                duplicate_frames: 0,
                muted: false,
                heartbeats: 0,
            });
        }
        let sockets: Vec<Arc<UdpSocket>> = units.iter().map(|u| Arc::clone(&u.socket)).collect();
        let chain = Arc::new(Mutex::new(Chain { units, boot: now }));
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
    pub fn addr(&self) -> SocketAddrV6 {
        self.group
    }

    #[must_use]
    pub fn interface(&self) -> Interface {
        Interface::Addr(self.group)
    }

    pub fn with_device<R>(&self, index: usize, f: impl FnOnce(&mut Device) -> R) -> R {
        f(&mut lock(&self.chain).units[index].device)
    }

    pub fn with_devices<R>(&self, f: impl FnOnce(&[&Device]) -> R) -> R {
        let chain = lock(&self.chain);
        let devices: Vec<&Device> = chain.units.iter().map(|unit| &unit.device).collect();
        f(&devices)
    }

    pub fn set_ptp_lock_blocked(&self, index: usize, blocked: bool) {
        let mut chain = lock(&self.chain);
        chain.units[index].lock_blocked = blocked;
        if blocked {
            if !chain.units[index].grandmaster {
                chain.units[index].unlock();
            }
        } else if chain.units[index].locked_at.is_none() {
            chain.follow_grandmaster(index, Instant::now());
        }
    }

    #[must_use]
    pub fn ptp_config(&self, index: usize) -> PtpConfig {
        lock(&self.chain).units[index].ptp_config
    }

    pub fn drop_next_frames(&self, index: usize, count: u32) {
        lock(&self.chain).units[index].drop_frames = count;
    }

    pub fn drop_next_replies(&self, index: usize, count: u32) {
        lock(&self.chain).units[index].drop_replies = count;
    }

    pub fn duplicate_next_frames(&self, index: usize, count: u32) {
        lock(&self.chain).units[index].duplicate_frames = count;
    }

    pub fn set_muted(&self, index: usize, muted: bool) {
        lock(&self.chain).units[index].muted = muted;
    }

    #[must_use]
    pub fn heartbeats_received(&self, index: usize) -> u64 {
        lock(&self.chain).units[index].heartbeats
    }

    pub fn reboot(&self, index: usize) {
        let mut chain = lock(&self.chain);
        let unit = &mut chain.units[index];
        unit.device.reset_cpu();
        unit.forget_host();
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

#[cfg(test)]
mod tests {
    use autd3_cpu_wire::config::CpuConfig;
    use autd3_cpu_wire::payload::SetCpuConfigPayload;
    use autd3_cpu_wire::{Cmd, Error, Telemetry};

    use super::*;

    const HOST: SocketAddr = SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 9, 0, 0));

    fn datagram(kind: Kind, body: &[u8]) -> Vec<u8> {
        [Header::new(kind, 1).as_bytes(), body].concat()
    }

    fn deliver(chain: &mut Chain, index: usize, seq: u8, cmd: Cmd, payload: &[u8]) {
        let body = [&[seq, cmd.as_u8()], payload].concat();
        chain.handle(Via::Unit(index), HOST, &datagram(Kind::Frame, &body));
        assert_eq!(chain.units[index].device.reply().status, Error::None);
    }

    fn configure(chain: &mut Chain, index: usize, failsafe_timeout: Option<Duration>) {
        for i in 0..=index {
            if chain.units[i].id.is_none() {
                let assign = AssignIdBody {
                    unit_id: u8::try_from(i).unwrap(),
                    role: Role::Slave.as_u8(),
                };
                chain.handle(
                    Via::Unit(i),
                    HOST,
                    &datagram(Kind::AssignId, assign.as_bytes()),
                );
                chain.handle(Via::Unit(i), HOST, &datagram(Kind::UnblockDownstream, &[]));
            }
        }
        deliver(chain, index, 0, Cmd::Reset, &[]);
        let config = CpuConfig {
            failsafe_timeout,
            ..CpuConfig::default()
        };
        let payload = SetCpuConfigPayload::encode(&config).unwrap();
        deliver(chain, index, 0, Cmd::SetCpuConfig, payload.as_bytes());
    }

    fn failsafe(chain: &Chain, index: usize) -> bool {
        chain.units[index].device.fpga().failsafe()
    }

    fn trips(chain: &Chain, index: usize) -> u32 {
        chain.units[index].device.cpu.telemetry(Telemetry::Failsafe)
    }

    fn last_host(chain: &Chain, index: usize) -> Instant {
        chain.units[index].last_host.unwrap()
    }

    #[test]
    fn a_two_second_failsafe_trips_after_three_silent_seconds() {
        let emulator = UdpEmulator::spawn(1).unwrap();
        let mut chain = lock(&emulator.chain);
        configure(&mut chain, 0, Some(Duration::from_secs(2)));
        let seen = last_host(&chain, 0);

        chain.idle(0, seen + Duration::from_millis(1999));
        assert!(!failsafe(&chain, 0));
        assert_eq!(trips(&chain, 0), 0);

        chain.idle(0, seen + Duration::from_secs(3));
        assert!(failsafe(&chain, 0));
        assert_eq!(trips(&chain, 0), 1);

        chain.idle(0, seen + Duration::from_secs(4));
        assert_eq!(trips(&chain, 0), 1);
    }

    #[test]
    fn a_single_long_silence_trips_the_failsafe_in_one_step() {
        let emulator = UdpEmulator::spawn(1).unwrap();
        let mut chain = lock(&emulator.chain);
        configure(&mut chain, 0, Some(Duration::from_secs(2)));
        let seen = last_host(&chain, 0);

        chain.idle(0, seen + Duration::from_secs(3));
        assert!(failsafe(&chain, 0));
        assert_eq!(trips(&chain, 0), 1);
    }

    #[test]
    fn a_muted_device_trips_the_failsafe_even_while_the_host_keeps_sending() {
        let emulator = UdpEmulator::spawn(2).unwrap();
        let mut chain = lock(&emulator.chain);
        configure(&mut chain, 0, Some(Duration::from_secs(2)));
        configure(&mut chain, 1, Some(Duration::from_secs(2)));
        chain.units[1].muted = true;
        let seen = last_host(&chain, 1);

        for elapsed_ms in [500, 1000, 1500, 2500, 3000] {
            let now = seen + Duration::from_millis(elapsed_ms);
            let header = Header::new(Kind::Heartbeat, 2);
            chain.heartbeat(0, HOST, header, now);
            chain.heartbeat(1, HOST, header, now);
        }
        assert!(!failsafe(&chain, 0));
        assert_eq!(trips(&chain, 0), 0);
        assert!(failsafe(&chain, 1));
        assert_eq!(trips(&chain, 1), 1);
    }

    #[test]
    fn a_heartbeat_restarts_the_silence() {
        let emulator = UdpEmulator::spawn(1).unwrap();
        let mut chain = lock(&emulator.chain);
        configure(&mut chain, 0, Some(Duration::from_secs(2)));
        let seen = last_host(&chain, 0);

        let beat = seen + Duration::from_millis(1500);
        chain.heartbeat(0, HOST, Header::new(Kind::Heartbeat, 2), beat);
        chain.idle(0, seen + Duration::from_secs(3));
        assert!(!failsafe(&chain, 0));

        chain.idle(0, beat + Duration::from_secs(2));
        assert!(failsafe(&chain, 0));
        assert_eq!(trips(&chain, 0), 1);
    }

    #[test]
    fn the_failsafe_stays_quiet_until_the_host_is_seen_and_after_a_reboot() {
        let emulator = UdpEmulator::spawn(1).unwrap();
        let booted = lock(&emulator.chain).boot;
        {
            let mut chain = lock(&emulator.chain);
            chain.idle(0, booted + Duration::from_secs(3));
            assert!(!failsafe(&chain, 0));
            configure(&mut chain, 0, Some(Duration::from_secs(2)));
        }
        emulator.reboot(0);
        let mut chain = lock(&emulator.chain);
        assert!(chain.units[0].last_host.is_none());
        chain.idle(0, booted + Duration::from_secs(10));
        assert!(!failsafe(&chain, 0));
        assert_eq!(trips(&chain, 0), 0);
    }

    #[test]
    fn a_stall_longer_than_the_catch_up_limit_still_trips_the_failsafe() {
        let emulator = UdpEmulator::spawn(1).unwrap();
        let mut chain = lock(&emulator.chain);
        configure(&mut chain, 0, Some(Duration::from_secs(2)));
        let seen = last_host(&chain, 0);

        chain.idle(0, seen + Duration::from_secs(600));
        assert!(failsafe(&chain, 0));
        assert_eq!(trips(&chain, 0), 1);
        assert_eq!(chain.units[0].last_tick, seen + Duration::from_secs(600));
    }

    #[test]
    fn the_receive_threads_advance_time_without_any_traffic() {
        let emulator = UdpEmulator::spawn(1).unwrap();
        configure(
            &mut lock(&emulator.chain),
            0,
            Some(Duration::from_millis(50)),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while !emulator.with_device(0, |device| device.fpga().failsafe()) {
            assert!(Instant::now() < deadline, "the failsafe never tripped");
            std::thread::sleep(POLL_INTERVAL);
        }
        assert_eq!(trips(&lock(&emulator.chain), 0), 1);
    }
}
