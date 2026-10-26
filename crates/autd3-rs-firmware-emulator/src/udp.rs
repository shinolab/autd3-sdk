use std::io;
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use autd3_cpu_fw::proto::{Drained, FrameHeader, Reply};
use autd3_cpu_wire::udp::{
    AssignIdBody, Flags, FrameReply, Header, Kind, PROTOCOL_VERSION, Role, SetTimeBody, Status,
    UNASSIGNED_ID, UnblockReply, UnitInfo,
};
use autd3_rs_core::geometry::Autd3;
use autd3_rs_core::{FRAME_BYTES_MAX, Interface};
use zerocopy::little_endian::{I32, U64};
use zerocopy::{FromBytes, IntoBytes};

use crate::Device;

const LOOPBACK_ANY: SocketAddrV6 = SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const MAX_CATCH_UP_TICKS: u128 = 60_000;
const NS_PER_SEC: u64 = 1_000_000_000;

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
    open: bool,
    lock_blocked_since: Option<Instant>,
    last_tick: Instant,
    host: Option<SocketAddr>,
    last_host_frame_ms: Option<u32>,
    main_loop_paused: bool,
    drop_frames: u32,
    drop_replies: u32,
    drop_unit_infos: u32,
    duplicate_frames: u32,
    muted: bool,
    heartbeats: u64,
    frames: u64,
}

impl Unit {
    fn forget(&mut self) {
        self.id = None;
        self.grandmaster = false;
        self.open = false;
        self.host = None;
    }
}

fn millis_since(at: Instant, now: Instant) -> u32 {
    u32::try_from(now.saturating_duration_since(at).as_millis()).unwrap_or(u32::MAX)
}

fn wrapping_millis_since(at: Instant, now: Instant) -> u32 {
    let ms = now.saturating_duration_since(at).as_millis();
    u32::try_from(ms & u128::from(u32::MAX)).unwrap_or(u32::MAX)
}

struct Chain {
    units: Vec<Unit>,
    boot: Instant,
    clock_offset_ns: Option<i64>,
}

impl Chain {
    fn host_now_ns(&self) -> i64 {
        i64::try_from(self.boot.elapsed().as_nanos()).unwrap_or(i64::MAX)
    }

    fn device_now(&self) -> u64 {
        u64::try_from(
            self.host_now_ns()
                .saturating_add(self.clock_offset_ns.unwrap_or(0)),
        )
        .unwrap_or(0)
    }

    fn assigned_after_time_set(&self, index: usize) -> bool {
        self.units[index].id.is_some() && self.clock_offset_ns.is_some()
    }

    fn locked(&self, index: usize) -> bool {
        let unit = &self.units[index];
        self.assigned_after_time_set(index)
            && (unit.grandmaster || unit.lock_blocked_since.is_none())
    }

    fn ptp_unlocked_ms(&self, index: usize, now: Instant) -> Option<u32> {
        let unit = &self.units[index];
        if !self.assigned_after_time_set(index) || unit.grandmaster {
            return None;
        }
        unit.lock_blocked_since.map(|at| millis_since(at, now))
    }

    fn reachable_end(&self) -> usize {
        self.units
            .iter()
            .position(|u| !u.open)
            .map_or(self.units.len(), |closed| closed + 1)
    }

    fn targets(&self, via: Via) -> Range<usize> {
        let end = self.reachable_end();
        match via {
            Via::Group => 0..end,
            Via::Unit(i) if i < end => i..i + 1,
            Via::Unit(_) => 0..0,
        }
    }

    fn flags(&self, index: usize) -> Flags {
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
        if self.locked(index) {
            flags |= Flags::PTP_LOCKED | Flags::SYNC_READY;
        }
        if unit.grandmaster {
            flags |= Flags::GRANDMASTER;
        }
        if unit.device.cpu.ptp_failsafe_active() {
            flags |= Flags::PTP_FAILSAFE;
        }
        flags
    }

    fn reply(&self, index: usize, dst: SocketAddr, header: Header, body_parts: &[&[u8]]) {
        let mut datagram = header.as_bytes().to_vec();
        for part in body_parts {
            datagram.extend_from_slice(part);
        }
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
        self.reply(index, dst, header, &[&[status.as_u8()], extra]);
    }

    fn unit_info(&self, index: usize) -> UnitInfo {
        let unit = &self.units[index];
        UnitInfo {
            unit_id: unit.id.unwrap_or(UNASSIGNED_ID),
            flags: self.flags(index),
            ptp_offset_ns: I32::new(0),
        }
    }

    fn handle(&mut self, via: Via, src: SocketAddr, datagram: &[u8]) {
        let now = Instant::now();
        let Ok((header, body)) = Header::read_from_prefix(datagram) else {
            return;
        };
        let targets = self.targets(via);
        if header.version != PROTOCOL_VERSION {
            let reply = Header {
                version: PROTOCOL_VERSION,
                ..header
            };
            for i in targets {
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
                    let info = self.unit_info(i);
                    self.reply_status(i, src, header, Status::Ok, info.as_bytes());
                }
                Kind::ResetId => {
                    self.reply_status(i, src, header, Status::Ok, &[]);
                    self.units[i].forget();
                }
                Kind::Discover => {
                    if self.units[i].id.is_none() {
                        self.reply_status(i, src, header, Status::Ok, &[]);
                    }
                }
                Kind::AssignId => self.assign(i, src, header, body),
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
                Kind::SetTime => self.set_time(i, src, header, body),
                _ => unreachable!(),
            }
        }
    }

    fn assign(&mut self, index: usize, src: SocketAddr, header: Header, body: &[u8]) {
        if self.units[index].id.is_some() {
            return;
        }
        let Some((request, role)) = AssignIdBody::read_from_prefix(body)
            .ok()
            .map(|(request, _)| request)
            .filter(|request| request.unit_id != UNASSIGNED_ID)
            .and_then(|request| Role::from_u8(request.role).map(|role| (request, role)))
        else {
            self.reply_status(index, src, header, Status::InvalidPayload, &[]);
            return;
        };
        let unit = &mut self.units[index];
        unit.id = Some(request.unit_id);
        unit.grandmaster = role == Role::Grandmaster;
        self.reply_status(index, src, header, Status::Ok, &[]);
    }

    fn set_time(&mut self, index: usize, src: SocketAddr, header: Header, body: &[u8]) {
        let status = if self.units[index].id.is_none() {
            Status::NotAssigned
        } else if !self.units[index].grandmaster {
            Status::NotGrandmaster
        } else if let Ok((request, _)) = SetTimeBody::read_from_prefix(body)
            && u32::try_from(request.sys_time.get() / NS_PER_SEC).is_ok()
        {
            let target = i64::try_from(request.sys_time.get()).unwrap_or(i64::MAX);
            self.clock_offset_ns = Some(target.saturating_sub(self.host_now_ns()));
            Status::Ok
        } else {
            Status::InvalidPayload
        };
        self.reply_status(index, src, header, status, &[]);
    }

    fn advance(&mut self, index: usize, now: Instant) {
        let bus = self.device_now();
        let now_ms = wrapping_millis_since(self.boot, now);
        let ptp_unlocked_ms = self.ptp_unlocked_ms(index, now);
        let unit = &mut self.units[index];
        let resets = unit.device.fpga().reset_count();
        unit.device.cpu.publish_host_idle(
            unit.last_host_frame_ms
                .map(|last| now_ms.wrapping_sub(last)),
        );
        unit.device.cpu.publish_ptp_unlocked(ptp_unlocked_ms);
        let elapsed = now.saturating_duration_since(unit.last_tick).as_millis();
        if elapsed > 0 && !unit.main_loop_paused {
            unit.last_tick = now;
            for _ in 0..elapsed.min(MAX_CATCH_UP_TICKS) {
                unit.device.tick_1ms();
            }
        }
        if unit.device.fpga().reset_count() != resets {
            unit.last_host_frame_ms = None;
            unit.forget();
        }
        unit.device.fpga_mut().update_with_sys_time(bus);
    }

    fn complete_one(&mut self, index: usize) -> bool {
        let Drained::Completed { msg_id } = self.units[index].device.process_one() else {
            return false;
        };
        if self.units[index].drop_replies > 0 {
            self.units[index].drop_replies -= 1;
        } else if let Some(host) = self.units[index].host {
            self.frame_reply(index, host, Header::new(Kind::Frame, msg_id));
        }
        true
    }

    fn resume_main_loop(&mut self, index: usize, now: Instant) {
        self.units[index].main_loop_paused = false;
        self.advance(index, now);
        while self.complete_one(index) {}
    }

    fn enqueue(&mut self, index: usize, body: &[u8], msg_id: u16) {
        self.units[index].device.recv(body, msg_id);
        if self.units[index].duplicate_frames > 0 {
            self.units[index].duplicate_frames -= 1;
            self.units[index].device.recv(body, msg_id);
        }
    }

    fn frame_reply(&self, index: usize, dst: SocketAddr, header: Header) {
        let Some(unit_id) = self.units[index].id else {
            return;
        };
        let state: Reply = self.units[index].device.reply();
        let head = FrameReply {
            reserved: 0,
            status: state.status.as_u8(),
            flags: self.flags(index),
            unit_id,
            sys_time: U64::new(self.device_now()),
        };
        self.reply(index, dst, header, &[head.as_bytes(), state.data()]);
    }

    fn note_host_contact(&mut self, index: usize, src: SocketAddr, now: Instant) -> bool {
        if self.units[index].muted {
            self.advance(index, now);
            return false;
        }
        self.units[index].last_host_frame_ms = Some(wrapping_millis_since(self.boot, now));
        self.units[index].host = Some(src);
        self.advance(index, now);
        true
    }

    fn heartbeat(&mut self, index: usize, src: SocketAddr, header: Header, now: Instant) {
        if self.units[index].id.is_none() {
            return;
        }
        self.units[index].heartbeats += 1;
        if !self.note_host_contact(index, src, now) {
            return;
        }
        self.frame_reply(index, src, header);
    }

    fn frame(&mut self, index: usize, src: SocketAddr, header: Header, frame: &[u8], now: Instant) {
        if self.units[index].id.is_none()
            || !(size_of::<FrameHeader>()..=FRAME_BYTES_MAX).contains(&frame.len())
        {
            return;
        }
        self.units[index].frames += 1;
        let mut immediate = [0u8; FRAME_BYTES_MAX];
        let body = &mut immediate[..frame.len()];
        body.copy_from_slice(frame);
        if let Ok((frame_header, _)) = FrameHeader::mut_from_prefix(body) {
            frame_header.exec_at = 0.into();
        }
        let body = &*body;
        if self.units[index].drop_frames > 0 {
            self.units[index].drop_frames -= 1;
            return;
        }
        if !self.note_host_contact(index, src, now) {
            return;
        }
        if self.units[index].id.is_none() {
            return;
        }
        self.enqueue(index, body, header.msg_id.get());
        if self.units[index].main_loop_paused {
            return;
        }
        let silent = self.units[index].drop_replies > 0;
        if silent {
            self.units[index].drop_replies -= 1;
        }
        loop {
            match self.units[index].device.process_one() {
                Drained::Empty => break,
                Drained::Completed { .. } if silent => {}
                Drained::Completed { msg_id } => {
                    let header = Header {
                        msg_id: zerocopy::little_endian::U16::new(msg_id),
                        ..header
                    };
                    self.frame_reply(index, src, header);
                }
            }
        }
    }
}

pub struct UdpEmulator {
    chain: Arc<Mutex<Chain>>,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    group: SocketAddrV6,
    device_addrs: Vec<SocketAddrV6>,
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
                    lock(chain).advance(index, Instant::now());
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

    pub fn spawn_simulator(port: u16, num_devices: usize) -> io::Result<Self> {
        Self::spawn_at(
            SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0),
            num_devices,
        )
    }

    fn spawn_at(group: SocketAddrV6, num_devices: usize) -> io::Result<Self> {
        let (group_socket, group) = bind(group)?;
        let now = Instant::now();
        let mut units = Vec::with_capacity(num_devices);
        let mut endpoints = vec![(group_socket, Via::Group)];
        let mut device_addrs = Vec::with_capacity(num_devices);
        for index in 0..num_devices {
            let (socket, addr) = bind(LOOPBACK_ANY)?;
            endpoints.push((Arc::clone(&socket), Via::Unit(index)));
            device_addrs.push(addr);
            units.push(Unit {
                device: Device::new(Autd3::NUM_TRANSDUCERS),
                socket,
                id: None,
                grandmaster: false,
                open: false,
                lock_blocked_since: None,
                last_tick: now,
                host: None,
                last_host_frame_ms: None,
                main_loop_paused: false,
                drop_frames: 0,
                drop_replies: 0,
                drop_unit_infos: 0,
                duplicate_frames: 0,
                muted: false,
                heartbeats: 0,
                frames: 0,
            });
        }
        let chain = Arc::new(Mutex::new(Chain {
            units,
            boot: now,
            clock_offset_ns: None,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let mut threads = Vec::with_capacity(num_devices + 1);
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
            device_addrs,
        })
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.device_addrs.len()
    }

    #[must_use]
    pub fn device_addr(&self, index: usize) -> SocketAddrV6 {
        self.device_addrs[index]
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
        let since = &mut chain.units[index].lock_blocked_since;
        if blocked {
            since.get_or_insert_with(Instant::now);
        } else {
            *since = None;
        }
    }

    pub fn pause_main_loop(&self, index: usize) {
        lock(&self.chain).units[index].main_loop_paused = true;
    }

    pub fn resume_main_loop(&self, index: usize) {
        lock(&self.chain).resume_main_loop(index, Instant::now());
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
    pub fn frames_received(&self, index: usize) -> u64 {
        lock(&self.chain).units[index].frames
    }

    pub fn reboot(&self, index: usize) {
        let mut chain = lock(&self.chain);
        let unit = &mut chain.units[index];
        unit.device.reset_cpu();
        unit.last_host_frame_ms = None;
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
