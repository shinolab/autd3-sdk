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
const NS_PER_SEC: u64 = 1_000_000_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Via {
    Group,
    Unit(usize),
}

struct Unit {
    device: Device,
    socket: Arc<UdpSocket>,
    retired: Option<Arc<UdpSocket>>,
    id: Option<u8>,
    grandmaster: bool,
    clock_offset_ns: i64,
    open: bool,
    close_at: Option<Instant>,
    locked_at: Option<Instant>,
    unlocked_since: Option<Instant>,
    lock_blocked: bool,
    ptp_config: PtpConfig,
    last_tick: Instant,
    last_host: Option<Instant>,
    host: Option<SocketAddr>,
    main_loop_paused: bool,
    drop_frames: u32,
    drop_replies: u32,
    drop_unit_infos: u32,
    duplicate_frames: u32,
    muted: bool,
    heartbeats: u64,
    id_resets: u64,
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
        self.unlocked_since = None;
        self.host = None;
        self.readdress();
    }

    fn readdress(&mut self) {
        match bind(LOOPBACK_ANY) {
            Ok((socket, _)) => {
                self.retired = Some(core::mem::replace(&mut self.socket, socket));
            }
            Err(e) => tracing::debug!("emulated device keeps its address: {e}"),
        }
    }

    fn lose_lock(&mut self, now: Instant) {
        if self.locked_at.is_some_and(|at| at <= now) {
            self.unlocked_since = Some(now);
        }
        self.unlock();
    }

    fn ptp_unlocked_ms(&self, now: Instant) -> Option<u32> {
        if self.locked_at.is_some_and(|at| at <= now) {
            return None;
        }
        self.unlocked_since.map(|at| {
            u32::try_from(now.saturating_duration_since(at).as_millis()).unwrap_or(u32::MAX)
        })
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
                unit.device.cpu.request_config_reset();
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
        let unit = &self.units[index];
        if unit.socket.send_to(&datagram, dst).is_err() {
            unit.device.cpu.count_send_failures(1);
        }
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

    fn handle(&mut self, via: Via, socket: &Arc<UdpSocket>, src: SocketAddr, datagram: &[u8]) {
        let now = Instant::now();
        self.apply_pending(now);
        if let Via::Unit(index) = via
            && !Arc::ptr_eq(&self.units[index].socket, socket)
        {
            return;
        }
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
            Kind::Frame | Kind::AssignId | Kind::UnblockDownstream | Kind::SetTime
        );
        if unicast_only && via == Via::Group {
            return;
        }
        for i in targets {
            match kind {
                Kind::Frame => self.frame(i, src, header, body, now),
                Kind::Heartbeat => self.heartbeat(i, src, header, now),
                Kind::ReadUnitInfo => {
                    if self.units[i].drop_unit_infos > 0 {
                        self.units[i].drop_unit_infos -= 1;
                        continue;
                    }
                    let info = self.unit_info(i, now);
                    self.reply_status(i, src, header, Status::Ok, info.as_bytes());
                }
                Kind::ResetId => {
                    self.units[i].id_resets += 1;
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
        unit.close_at = None;
        unit.readdress();
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
        } else if let Ok((request, _)) = SetTimeBody::read_from_prefix(body)
            && u32::try_from(request.sys_time.get() / NS_PER_SEC).is_ok()
        {
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
        unit.device.fpga_mut().ptp_unlocked_ms = unit.ptp_unlocked_ms(now);
        let elapsed = now.saturating_duration_since(unit.last_tick).as_millis();
        if elapsed > 0 && !unit.main_loop_paused {
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

    fn complete_one(&mut self, index: usize, now: Instant) -> bool {
        let Drained::Completed { msg_id } = self.units[index].device.process_one() else {
            return false;
        };
        self.apply_ptp_config(index, now);
        if self.units[index].drop_replies > 0 {
            self.units[index].drop_replies -= 1;
        } else if let Some(host) = self.units[index].host {
            self.frame_reply(index, host, Header::new(Kind::Frame, msg_id), now);
        }
        true
    }

    fn run_main_loop(&mut self, index: usize, now: Instant, frames: usize) -> usize {
        self.apply_pending(now);
        let paused = core::mem::replace(&mut self.units[index].main_loop_paused, false);
        self.advance(index, now);
        self.units[index].main_loop_paused = paused;
        (0..frames)
            .take_while(|_| self.complete_one(index, now))
            .count()
    }

    fn frame_while_paused(
        &mut self,
        index: usize,
        src: SocketAddr,
        header: Header,
        body: &[u8],
        now: Instant,
    ) {
        let msg_id = header.msg_id.get();
        let mut answer = self.units[index].device.recv(body, msg_id) == Disposition::Reply;
        if self.units[index].duplicate_frames > 0 {
            self.units[index].duplicate_frames -= 1;
            answer |= self.units[index].device.recv(body, msg_id) == Disposition::Reply;
        }
        if !answer {
            return;
        }
        if self.units[index].drop_replies > 0 {
            self.units[index].drop_replies -= 1;
            return;
        }
        self.frame_reply(index, src, header, now);
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
        self.units[index].host = Some(src);
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
        self.units[index].host = Some(src);
        self.advance(index, now);
        if self.units[index].id.is_none() {
            return;
        }
        if self.units[index].main_loop_paused {
            self.frame_while_paused(index, src, header, body, now);
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

fn serve(chain: &Mutex<Chain>, socket: &Arc<UdpSocket>, via: Via, stop: &AtomicBool) {
    let mut buf = vec![0u8; 2048];
    while !stop.load(Ordering::Acquire) {
        let socket = match via {
            Via::Group => Arc::clone(socket),
            Via::Unit(index) => Arc::clone(&lock(chain).units[index].socket),
        };
        match socket.recv_from(&mut buf) {
            Ok((len, src)) => lock(chain).handle(via, &socket, src, &buf[..len]),
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
                retired: None,
                id: None,
                grandmaster: false,
                clock_offset_ns: 0,
                open: false,
                close_at: None,
                locked_at: None,
                unlocked_since: None,
                lock_blocked: false,
                ptp_config: PtpConfig::default(),
                last_tick: now,
                last_host: None,
                host: None,
                main_loop_paused: false,
                drop_frames: 0,
                drop_replies: 0,
                drop_unit_infos: 0,
                duplicate_frames: 0,
                muted: false,
                heartbeats: 0,
                id_resets: 0,
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
                chain.units[index].lose_lock(Instant::now());
            }
        } else if chain.units[index].locked_at.is_none() {
            chain.follow_grandmaster(index, Instant::now());
        }
    }

    #[must_use]
    pub fn ptp_config(&self, index: usize) -> PtpConfig {
        lock(&self.chain).units[index].ptp_config
    }

    pub fn pause_main_loop(&self, index: usize) {
        lock(&self.chain).units[index].main_loop_paused = true;
    }

    pub fn resume_main_loop(&self, index: usize) {
        let mut chain = lock(&self.chain);
        chain.units[index].main_loop_paused = false;
        chain.run_main_loop(index, Instant::now(), usize::MAX);
    }

    #[must_use]
    pub fn step_main_loop(&self, index: usize) -> bool {
        lock(&self.chain).run_main_loop(index, Instant::now(), 1) == 1
    }

    pub fn drop_next_frames(&self, index: usize, count: u32) {
        lock(&self.chain).units[index].drop_frames = count;
    }

    pub fn drop_next_replies(&self, index: usize, count: u32) {
        lock(&self.chain).units[index].drop_replies = count;
    }

    pub fn drop_next_unit_infos(&self, index: usize, count: u32) {
        lock(&self.chain).units[index].drop_unit_infos = count;
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

    #[must_use]
    pub fn id_resets_received(&self, index: usize) -> u64 {
        lock(&self.chain).units[index].id_resets
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
