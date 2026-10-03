mod frame;
mod master;
mod raw;
mod reg;
#[cfg(test)]
mod sim;
mod tuning;

use std::io;
use std::time::{Duration, Instant};

use autd3_rs::protocol::{Cmd, FRAME_HEADER_BYTES, Seq};
use autd3_rs::value::SysTimeError;

use crate::driver::{DeviceReply, Dialect, Exchange, Frame, LEGACY_PAYLOAD_BYTES, Replies};
use frame::FrameError;
use master::{Master, MasterConfig};
use raw::{PERMISSION_HINT, RawBus, RawSocket};
use reg::AlState;
use tuning::PerfTuning;

pub const MIN_ETHERCAT_CPU_FIRMWARE_VERSION: (u8, u8, u8) = (0, 9, 0);

pub(crate) const LEGACY_FRAME_BYTES: usize = master::OUTPUT_BYTES as usize;
pub(crate) const LEGACY_REPLY_BYTES: usize = master::INPUT_BYTES as usize;
const RESET_CYCLES: u32 = 2;

const _: () = assert!(LEGACY_FRAME_BYTES == FRAME_HEADER_BYTES + LEGACY_PAYLOAD_BYTES);

#[derive(Debug, thiserror::Error)]
pub enum EcatError {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Frame(#[from] FrameError),
    #[error("EtherCAT is unavailable on this host: {PERMISSION_HINT}")]
    Unavailable,
    #[error("no response to an EtherCAT frame within {0:?}")]
    Timeout(Duration),
    #[error("working counter mismatch: expected {expected}, received {received}")]
    WorkingCounter { expected: u16, received: u16 },
    #[error("no EtherCAT device responded on the selected interface")]
    NoSubDevices,
    #[error("no interface has an EtherCAT device attached")]
    NoInterfaceFound,
    #[error("cannot open {interface} for EtherCAT: {source}")]
    InterfaceUnavailable {
        interface: String,
        #[source]
        source: io::Error,
    },
    #[error("cannot open a raw socket on {}: {PERMISSION_HINT}", .interface.as_deref().unwrap_or("any interface"))]
    PermissionDenied { interface: Option<String> },
    #[error(
        "EtherCAT device {index} is not an AUTD3 (vendor {vendor:#010x}, product {product:#010x})"
    )]
    ForeignSubDevice {
        index: usize,
        vendor: u32,
        product: u32,
    },
    #[error("the EtherCAT device count changed from {expected} to {received} during startup")]
    SubDeviceCountChanged { expected: usize, received: usize },
    #[error("{expected} devices were expected, but {found} are on the EtherCAT bus")]
    DeviceCountMismatch { expected: usize, found: usize },
    #[error("SII read from EtherCAT device {index} at word {word:#06x} timed out")]
    SiiTimeout { index: usize, word: u16 },
    #[error(
        "EtherCAT device {index} refused the transition to {target:?}: AL status {status:?}, code {code:#06x} ({})",
        reg::al_status_code_str(*code)
    )]
    AlTransition {
        index: usize,
        target: AlState,
        status: Option<AlState>,
        code: u16,
    },
    #[error("the EtherCAT bus did not reach {target:?} within {timeout:?}")]
    AlTimeout { target: AlState, timeout: Duration },
    #[error("distributed clocks did not settle within {0:?}")]
    DcSyncTimeout(Duration),
    #[error("propagation delay to EtherCAT device {index} measured as {delay_ns} ns")]
    ImplausiblePropagationDelay { index: usize, delay_ns: u32 },
    #[error("the EtherCAT connection is closed")]
    Closed,
    #[error("the host clock cannot be read as an EtherCAT system time: {0}")]
    HostClock(#[from] SysTimeError),
}

impl EcatError {
    #[must_use]
    pub fn found_nothing(&self) -> bool {
        matches!(
            self,
            Self::Unavailable
                | Self::NoSubDevices
                | Self::NoInterfaceFound
                | Self::InterfaceUnavailable { .. }
                | Self::PermissionDenied { .. }
        )
    }
}

pub struct EcatBus {
    master: Master<Box<dyn RawBus>>,
    interface: String,
    closed: bool,
    tx: Vec<u8>,
    rx: Vec<u8>,
    _tuning: Option<PerfTuning>,
}

fn open_error(e: io::Error, interface: &str) -> EcatError {
    if e.kind() == io::ErrorKind::PermissionDenied {
        EcatError::PermissionDenied {
            interface: Some(interface.to_owned()),
        }
    } else {
        EcatError::InterfaceUnavailable {
            interface: interface.to_owned(),
            source: e,
        }
    }
}

fn bring_up<B: RawBus>(mut master: Master<B>) -> Result<Master<B>, EcatError> {
    match master.probe() {
        Ok(0) | Err(EcatError::Timeout(_)) => Err(EcatError::NoSubDevices),
        Ok(_) => master.bring_up().map(|()| master),
        Err(e) => Err(e),
    }
}

fn open_socket(interface: &str) -> Result<Box<dyn RawBus>, EcatError> {
    RawSocket::open(interface)
        .map(|socket| Box::new(socket) as Box<dyn RawBus>)
        .map_err(|e| open_error(e, interface))
}

fn find_interface(
    candidates: &[String],
    open: impl Fn(&str) -> Result<Box<dyn RawBus>, EcatError>,
    config: MasterConfig,
) -> Result<String, EcatError> {
    let mut denied = 0;
    for name in candidates {
        let bus = match open(name) {
            Ok(bus) => bus,
            Err(EcatError::PermissionDenied { .. }) => {
                denied += 1;
                continue;
            }
            Err(_) => continue,
        };
        if matches!(Master::new(bus, config).probe(), Ok(found) if found > 0) {
            return Ok(name.clone());
        }
    }
    Err(if denied > 0 && denied == candidates.len() {
        EcatError::PermissionDenied { interface: None }
    } else {
        EcatError::NoInterfaceFound
    })
}

impl EcatBus {
    pub fn open(
        iface: Option<&str>,
        cycle: Option<Duration>,
        expected: Option<usize>,
    ) -> Result<Self, EcatError> {
        if !raw::available() {
            return Err(EcatError::Unavailable);
        }
        let default = MasterConfig::default();
        let config = MasterConfig {
            cycle: cycle.unwrap_or(default.cycle),
            ..default
        };
        let interface = match iface {
            Some(name) => name.to_owned(),
            None => find_interface(&raw::interface_candidates()?, open_socket, config)?,
        };
        let tuning = PerfTuning::apply();
        let master = bring_up(Master::new(open_socket(&interface)?, config))?;
        Self::checked(master, interface, expected, Some(tuning))
    }

    fn checked(
        master: Master<Box<dyn RawBus>>,
        interface: String,
        expected: Option<usize>,
        tuning: Option<PerfTuning>,
    ) -> Result<Self, EcatError> {
        let n = master.num_devices();
        let mut bus = Self {
            master,
            interface,
            closed: false,
            tx: vec![0; n * LEGACY_FRAME_BYTES],
            rx: vec![0; n * LEGACY_REPLY_BYTES],
            _tuning: tuning,
        };
        match expected {
            Some(expected) if expected != bus.master.num_devices() => {
                let found = bus.master.num_devices();
                let _ = bus.close();
                Err(EcatError::DeviceCountMismatch { expected, found })
            }
            _ => Ok(bus),
        }
    }

    #[must_use]
    pub fn interface(&self) -> &str {
        &self.interface
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.master.num_devices()
    }

    pub fn close(&mut self) -> Result<(), EcatError> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        self.master.close()
    }
}

impl Drop for EcatBus {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl EcatBus {
    fn stage(&mut self, seq: Seq, cmd: u8, payload: &[u8]) {
        for frame in self.tx.chunks_mut(LEGACY_FRAME_BYTES) {
            frame[0] = seq.get();
            frame[1] = cmd;
            frame[FRAME_HEADER_BYTES..].copy_from_slice(&payload[..LEGACY_PAYLOAD_BYTES]);
        }
    }

    fn cycle(&mut self) -> Result<bool, EcatError> {
        Ok(self.master.cycle(&self.tx, &mut self.rx)?.rx_valid)
    }
}

impl Exchange for EcatBus {
    type Error = EcatError;

    fn num_devices(&self) -> usize {
        EcatBus::num_devices(self)
    }

    fn min_cpu_firmware_version(&self) -> (u8, u8, u8) {
        MIN_ETHERCAT_CPU_FIRMWARE_VERSION
    }

    fn dialect(&self) -> Dialect {
        Dialect::Legacy
    }

    fn reset(&mut self, _timeout: Duration) -> Result<bool, EcatError> {
        self.stage(Seq::ZERO, Cmd::Reset.as_u8(), &[0; LEGACY_PAYLOAD_BYTES]);
        for _ in 0..RESET_CYCLES {
            self.cycle()?;
        }
        Ok(true)
    }

    fn exchange(
        &mut self,
        seq: Seq,
        frame: &Frame,
        timeout: Duration,
    ) -> Result<Replies, EcatError> {
        self.stage(seq, frame.cmd, &frame.payload);
        let mut replies: Vec<Option<DeviceReply>> = vec![None; self.num_devices()];
        let start = Instant::now();
        loop {
            if self.cycle()? {
                for (reply, rx) in replies.iter_mut().zip(self.rx.chunks(LEGACY_REPLY_BYTES)) {
                    if reply.is_none() && rx[0] == seq.get() {
                        *reply = Some(DeviceReply {
                            status: rx[1],
                            value: vec![rx[1]],
                        });
                    }
                }
                if replies.iter().all(Option::is_some) {
                    return Ok(Ok(replies.into_iter().map(Option::unwrap).collect()));
                }
            }
            if start.elapsed() >= timeout {
                return Ok(Err(replies.iter().position(Option::is_none).unwrap_or(0)));
            }
        }
    }

    fn idle(&mut self, duration: Duration) -> Result<(), EcatError> {
        let start = Instant::now();
        while start.elapsed() < duration {
            self.cycle()?;
        }
        Ok(())
    }

    fn close(&mut self) -> Result<(), EcatError> {
        EcatBus::close(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::EscSim;

    fn sim_config() -> MasterConfig {
        MasterConfig {
            cycle: Duration::from_millis(1),
            dc_static_sync_iterations: 32,
            dc_start_delay: Duration::from_millis(10),
            ..MasterConfig::default()
        }
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    fn denied(name: &str) -> EcatError {
        open_error(io::Error::from(io::ErrorKind::PermissionDenied), name)
    }

    pub(crate) fn sim_bus(sim: EscSim, expected: Option<usize>) -> Result<EcatBus, EcatError> {
        let master = bring_up(Master::new(Box::new(sim) as Box<dyn RawBus>, sim_config()))?;
        EcatBus::checked(master, "sim".to_owned(), expected, None)
    }

    #[test]
    fn the_first_interface_with_an_autd3_answering_is_chosen() {
        let found = find_interface(
            &names(&["eth0", "eth1", "eth2"]),
            |name| match name {
                "eth0" => Err(denied(name)),
                "eth1" => Ok(Box::new(EscSim::nop(0))),
                _ => Ok(Box::new(EscSim::nop(2))),
            },
            sim_config(),
        )
        .unwrap();
        assert_eq!(found, "eth2");
    }

    #[test]
    fn every_candidate_denied_is_a_permission_problem_not_a_missing_bus() {
        let e = find_interface(&names(&["eth0", "eth1"]), |n| Err(denied(n)), sim_config())
            .unwrap_err();
        assert!(matches!(e, EcatError::PermissionDenied { interface: None }));
        assert!(e.found_nothing());
    }

    #[test]
    fn an_interface_without_a_bus_is_nothing_found() {
        let e = find_interface(
            &names(&["eth0"]),
            |_| Ok(Box::new(EscSim::nop(0))),
            sim_config(),
        )
        .unwrap_err();
        assert!(matches!(e, EcatError::NoInterfaceFound));
        assert!(e.found_nothing());
    }

    #[test]
    fn the_bus_reaches_op_and_carries_process_data() {
        let mut bus = sim_bus(EscSim::nop(3), Some(3)).unwrap();
        assert!(bus.cycle().unwrap());
        assert_eq!(bus.min_cpu_firmware_version(), (0, 9, 0));
        assert_eq!(bus.dialect(), Dialect::Legacy);
        bus.close().unwrap();
        assert!(matches!(bus.cycle(), Err(EcatError::Closed)));
    }

    #[test]
    fn a_named_interface_without_a_bus_is_nothing_found() {
        let e = sim_bus(EscSim::nop(0), None).err().unwrap();
        assert!(matches!(e, EcatError::NoSubDevices));
        assert!(e.found_nothing());
        let missing = open_error(io::Error::from(io::ErrorKind::NotFound), "eth9");
        assert!(missing.found_nothing());
        assert!(missing.to_string().contains("eth9"));
    }

    #[test]
    fn a_device_count_other_than_the_expected_one_is_rejected() {
        assert!(matches!(
            sim_bus(EscSim::nop(2), Some(3)),
            Err(EcatError::DeviceCountMismatch {
                expected: 3,
                found: 2
            })
        ));
    }

    #[test]
    fn a_foreign_device_is_rejected() {
        assert!(matches!(
            sim_bus(EscSim::foreign(1), None),
            Err(EcatError::ForeignSubDevice { index: 0, .. })
        ));
    }

    #[test]
    fn a_device_that_drops_out_of_op_is_brought_back() {
        let sim = EscSim::nop(2);
        let mut bus = sim_bus(sim.clone(), None).unwrap();
        bus.cycle().unwrap();
        sim.latch_al_error(AlState::SafeOp, 0x001b);
        assert!(!sim.all_in(AlState::Op));
        let recovered = (0..16).any(|_| {
            bus.cycle().unwrap();
            sim.all_in(AlState::Op)
        });
        assert!(recovered);
    }

    struct Firmware(std::sync::Arc<std::sync::Mutex<autd3_rs_firmware_emulator::Device>>);

    fn legacy_to_current(outputs: &[u8]) -> Vec<u8> {
        use autd3_cpu_wire::payload::{SetModePayload, UpdateBeginPayload, UpdateChunkPayload};
        use zerocopy::{FromBytes, IntoBytes};

        use crate::driver::LegacyUpdateChunkPayload;

        let (header, payload) = outputs.split_at(FRAME_HEADER_BYTES);
        let mut frame = header.to_vec();
        match Cmd::from_u8(header[1]) {
            Some(Cmd::UpdateBegin) => {
                frame.extend_from_slice(&payload[..size_of::<UpdateBeginPayload>()]);
            }
            Some(Cmd::SetMode) => {
                frame.extend_from_slice(&payload[..size_of::<SetModePayload>()]);
            }
            Some(Cmd::UpdateChunk) => {
                let (chunk, data) = LegacyUpdateChunkPayload::ref_from_prefix(payload).unwrap();
                frame.extend_from_slice(
                    UpdateChunkPayload {
                        offset: chunk.offset,
                    }
                    .as_bytes(),
                );
                frame.extend_from_slice(&data[..usize::from(chunk.data_len.get())]);
            }
            _ => {}
        }
        frame
    }

    impl sim::ProcessData for Firmware {
        fn exchange(&mut self, outputs: &[u8], inputs: &mut [u8]) {
            let (seq, cmd) = (outputs[0], outputs[1]);
            let legacy_version = (0xE1..=0xE3).contains(&cmd);
            let reply = if legacy_version {
                self.0
                    .lock()
                    .unwrap()
                    .send(&[seq, Cmd::ReadFirmwareInfo.as_u8()])
            } else {
                self.0.lock().unwrap().send(&legacy_to_current(outputs))
            };
            inputs[0] = reply.ack;
            inputs[1] = if legacy_version && reply.ack == seq {
                reply.data()[usize::from(cmd - 0xE1)]
            } else {
                reply.status
            };
        }
    }

    #[test]
    fn a_udp_cpu_image_streams_to_every_device_over_ethercat() {
        use autd3_cpu_wire::update::{
            ImageHeader, Slot, TRANSPORT_MARKER_BYTES, TRANSPORT_MARKER_OFFSET, Transport,
        };
        use zerocopy::FromBytes;

        use crate::{CpuFirmwareImage, Driver};

        let devices: Vec<_> = (0..2)
            .map(|_| {
                std::sync::Arc::new(std::sync::Mutex::new(
                    autd3_rs_firmware_emulator::Device::new(249),
                ))
            })
            .collect();
        let sim = EscSim::with_process_data(2, |i| Box::new(Firmware(devices[i].clone())));
        let mut driver = Driver::open(sim_bus(sim, Some(2)).unwrap()).unwrap();
        assert_eq!(driver.read_cpu_version().unwrap().len(), 2);

        let mut body: Vec<u8> = (0..3000u32).map(|i| i.to_le_bytes()[0] ^ 0x5a).collect();
        let at = TRANSPORT_MARKER_OFFSET as usize;
        body[at..at + TRANSPORT_MARKER_BYTES].copy_from_slice(&Transport::Udp.marker());
        let image = CpuFirmwareImage::from_slot_image(body.clone()).unwrap();
        driver.update(&image, |_| {}).unwrap();
        driver.close().unwrap();

        for device in &devices {
            let device = device.lock().unwrap();
            let flash = device.fpga().cpu_flash();
            let base = Slot::B.base() as usize;
            let header = ImageHeader::read_from_bytes(
                &flash[base..base + core::mem::size_of::<ImageHeader>()],
            )
            .unwrap();
            assert!(header.is_trial());
            assert_eq!(header.length.get() as usize, body.len());
            let image_base = Slot::B.image_base() as usize;
            assert_eq!(&flash[image_base..image_base + body.len()], &body[..]);
        }
    }
}
