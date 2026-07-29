mod cyclic;
pub mod dc;
pub mod init;
mod open;
mod state;

pub use cyclic::{CycleReport, LOSE_CONTACT_AFTER_CYCLES, next_cycle_wait};
pub use state::{BusState, device_state};

use std::time::{Duration, Instant};

use crate::bus::RawBus;
use crate::error::EchocatError;
use crate::reg::{self, AlState};
use crate::wire::{Address, Command, FrameBuilder, FrameView, frame_bytes_for};

use cyclic::CyclePlan;

pub const FIRST_STATION_ADDRESS: u16 = 0x1000;

const PHASE_CORRECTION_DIVISOR: i64 = 4;

pub(crate) fn is_echo(received: &[u8], sent: &[u8]) -> bool {
    received == sent
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SleepStrategy {
    #[default]
    Sleep,
    Spin {
        margin: Duration,
    },
}

impl SleepStrategy {
    pub fn wait_until(self, deadline: Instant) {
        match self {
            Self::Sleep => {
                let now = Instant::now();
                if deadline > now {
                    std::thread::sleep(deadline - now);
                }
            }
            Self::Spin { margin } => {
                spin_sleep::SpinSleeper::new(u32::try_from(margin.as_nanos()).unwrap_or(u32::MAX))
                    .with_spin_strategy(spin_sleep::SpinStrategy::SpinLoopHint)
                    .sleep_until(deadline);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MasterConfig {
    pub cycle: Duration,
    pub pdu_timeout: Duration,
    pub state_transition_timeout: Duration,
    pub dc_static_sync_iterations: u32,
    pub dc_start_delay: Duration,
    pub sync_tolerance: Duration,
    pub sync_timeout: Duration,
    pub process_data_watchdog: Duration,
    pub sleep_strategy: SleepStrategy,
}

impl Default for MasterConfig {
    fn default() -> Self {
        Self {
            cycle: Duration::from_millis(1),
            pdu_timeout: Duration::from_millis(100),
            state_transition_timeout: Duration::from_secs(10),
            dc_static_sync_iterations: 10_000,
            dc_start_delay: Duration::from_millis(100),
            sync_tolerance: Duration::from_micros(1),
            sync_timeout: Duration::from_secs(10),
            process_data_watchdog: Duration::from_millis(100),
            sleep_strategy: SleepStrategy::Sleep,
        }
    }
}

pub struct Master<B: RawBus> {
    bus: B,
    tx: Vec<u8>,
    rx: Vec<u8>,
    index: u8,
    config: MasterConfig,
    devices: usize,
    plan: CyclePlan,
    next_at: Option<Instant>,
    last_cycle_at: Option<Instant>,
    op_entered: bool,
    rotation: usize,
    unobserved_cycles: u32,
    state: BusState,
    phase_bias_ns: i64,
    phase_excursions: u64,
    worst_phase_ns: u64,
    last_phase_warn_at: Option<Instant>,
}

impl<B: RawBus> Master<B> {
    pub fn new(bus: B, config: MasterConfig) -> Self {
        let mtu = bus.mtu();
        Self {
            bus,
            tx: vec![0; mtu + crate::wire::ETH_HEADER_BYTES],
            rx: vec![0; mtu + crate::wire::ETH_HEADER_BYTES],
            index: 0,
            config,
            devices: 0,
            plan: CyclePlan::default(),
            next_at: None,
            last_cycle_at: None,
            op_entered: false,
            rotation: 0,
            unobserved_cycles: 0,
            state: BusState::new(0),
            phase_bias_ns: 0,
            phase_excursions: 0,
            worst_phase_ns: 0,
            last_phase_warn_at: None,
        }
    }

    #[must_use]
    pub fn phase_bias(&self) -> Duration {
        Duration::from_nanos(self.phase_bias_ns.unsigned_abs())
    }

    pub(crate) fn update_phase_bias(&mut self, dc_system_time: u64) {
        let Some(cycle_ns) = i64::try_from(self.config.cycle.as_nanos()).ok() else {
            return;
        };
        if cycle_ns == 0 || dc_system_time == 0 {
            return;
        }
        let phase = i64::try_from(dc_system_time % cycle_ns.unsigned_abs()).expect("phase < cycle");
        let target = cycle_ns / 2;
        let error = phase - target;
        self.phase_bias_ns =
            (self.phase_bias_ns + error / PHASE_CORRECTION_DIVISOR).clamp(0, target);
    }

    #[must_use]
    pub fn state(&self) -> BusState {
        self.state.clone()
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.devices
    }

    #[must_use]
    pub fn config(&self) -> &MasterConfig {
        &self.config
    }

    #[must_use]
    pub fn station_address(index: usize) -> u16 {
        FIRST_STATION_ADDRESS + u16::try_from(index).expect("device index fits in u16")
    }

    pub fn bus_mut(&mut self) -> &mut B {
        &mut self.bus
    }

    #[must_use]
    pub fn bus(&self) -> &B {
        &self.bus
    }

    fn next_index(&mut self) -> u8 {
        self.index = self.index.wrapping_add(1);
        self.index
    }

    fn receive_matching(&mut self, index: u8, sent: usize) -> Result<usize, EchocatError> {
        let deadline = Instant::now() + self.config.pdu_timeout;
        let mut echo_pending = true;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(EchocatError::Timeout(self.config.pdu_timeout));
            }
            let Some(len) = self.bus.receive(&mut self.rx, deadline - now)? else {
                continue;
            };
            if echo_pending && is_echo(&self.rx[..len], &self.tx[..sent]) {
                tracing::trace!("discarding a frame the interface looped back");
                echo_pending = false;
                continue;
            }
            match FrameView::parse(&self.rx[..len], index) {
                Ok(_) => return Ok(len),
                Err(crate::wire::FrameError::IndexMismatch { .. }) => {
                    tracing::trace!("discarding a stale EtherCAT frame");
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn transact(
        &mut self,
        command: Command,
        address: Address,
        data: &mut [u8],
    ) -> Result<u16, EchocatError> {
        let index = self.next_index();
        let need = frame_bytes_for(&[data.len()]);
        if self.tx.len() < need {
            self.tx.resize(need, 0);
        }
        let mut builder = FrameBuilder::new(&mut self.tx[..need], index);
        let slot = builder.push(command, address, data.len())?;
        builder.data_mut(slot).copy_from_slice(data);
        let len = builder.finish();
        self.bus.send(&self.tx[..len])?;

        let len = self.receive_matching(index, len)?;
        let view = FrameView::parse(&self.rx[..len], index)?;
        data.copy_from_slice(view.data(slot)?);
        Ok(view.wkc(slot)?)
    }

    fn expect_wkc(wkc: u16, expected: u16) -> Result<(), EchocatError> {
        if wkc == expected {
            Ok(())
        } else {
            Err(EchocatError::WorkingCounter {
                expected,
                received: wkc,
            })
        }
    }

    pub(crate) fn read_bytes(
        &mut self,
        command: Command,
        address: Address,
        data: &mut [u8],
    ) -> Result<u16, EchocatError> {
        data.fill(0);
        self.transact(command, address, data)
    }

    pub(crate) fn write_bytes(
        &mut self,
        command: Command,
        address: Address,
        data: &[u8],
    ) -> Result<u16, EchocatError> {
        let mut scratch = data.to_vec();
        self.transact(command, address, &mut scratch)
    }

    pub(crate) fn read_u16(
        &mut self,
        command: Command,
        address: Address,
    ) -> Result<(u16, u16), EchocatError> {
        let mut data = [0u8; 2];
        let wkc = self.transact(command, address, &mut data)?;
        Ok((u16::from_le_bytes(data), wkc))
    }

    pub(crate) fn read_u32(
        &mut self,
        command: Command,
        address: Address,
    ) -> Result<(u32, u16), EchocatError> {
        let mut data = [0u8; 4];
        let wkc = self.transact(command, address, &mut data)?;
        Ok((u32::from_le_bytes(data), wkc))
    }

    pub(crate) fn read_u64(
        &mut self,
        command: Command,
        address: Address,
    ) -> Result<(u64, u16), EchocatError> {
        let mut data = [0u8; 8];
        let wkc = self.transact(command, address, &mut data)?;
        Ok((u64::from_le_bytes(data), wkc))
    }

    pub(crate) fn write_u8(
        &mut self,
        command: Command,
        address: Address,
        value: u8,
    ) -> Result<u16, EchocatError> {
        self.transact(command, address, &mut [value])
    }

    pub(crate) fn write_u16(
        &mut self,
        command: Command,
        address: Address,
        value: u16,
    ) -> Result<u16, EchocatError> {
        self.transact(command, address, &mut value.to_le_bytes())
    }

    pub(crate) fn write_u32(
        &mut self,
        command: Command,
        address: Address,
        value: u32,
    ) -> Result<u16, EchocatError> {
        self.transact(command, address, &mut value.to_le_bytes())
    }

    pub(crate) fn write_u64(
        &mut self,
        command: Command,
        address: Address,
        value: u64,
    ) -> Result<u16, EchocatError> {
        self.transact(command, address, &mut value.to_le_bytes())
    }

    pub fn al_states(&mut self) -> Result<Vec<(Option<AlState>, u16)>, EchocatError> {
        let mut observed = Vec::with_capacity(self.devices);
        for index in 0..self.devices {
            let node = Self::station_address(index);
            let (raw, _) = self.read_u16(Command::Fprd, Address::node(node, reg::AL_STATUS))?;
            let raw = u8::try_from(raw & 0xff).expect("masked");
            let code = if raw & AlState::ERROR_FLAG == 0 {
                0
            } else {
                self.read_u16(Command::Fprd, Address::node(node, reg::AL_STATUS_CODE))?
                    .0
            };
            observed.push((AlState::from_code(raw), code));
        }
        Ok(observed)
    }

    pub fn request_state(&mut self, target: AlState) -> Result<(), EchocatError> {
        self.write_al_control(target.code())?;
        self.wait_for_state(target)
    }

    pub fn acknowledge_errors_and_request_init(&mut self) -> Result<(), EchocatError> {
        let deadline = Instant::now() + self.config.state_transition_timeout;
        loop {
            self.write_al_control(AlState::Init.code() | AlState::ERROR_FLAG)?;
            let states = self.al_states()?;
            if states
                .iter()
                .all(|(status, code)| *status == Some(AlState::Init) && *code == 0)
            {
                return Ok(());
            }
            if let Some((index, (status, code))) = states
                .iter()
                .enumerate()
                .find(|(_, (_, code))| *code != 0)
                .map(|(index, entry)| (index, *entry))
            {
                tracing::debug!(
                    device = index,
                    ?status,
                    al_status_code = format_args!("{code:#06x}"),
                    reason = reg::al_status_code_str(code),
                    "the subdevice latched another error while going to INIT; acknowledging again",
                );
            }
            if Instant::now() >= deadline {
                return Err(EchocatError::AlTimeout {
                    target: AlState::Init,
                    timeout: self.config.state_transition_timeout,
                });
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn write_al_control(&mut self, control: u8) -> Result<(), EchocatError> {
        let expected = u16::try_from(self.devices).expect("device count fits in u16");
        let wkc = self.write_u16(
            Command::Bwr,
            Address::broadcast(reg::AL_CONTROL),
            u16::from(control),
        )?;
        Self::expect_wkc(wkc, expected)
    }

    pub fn wait_for_state(&mut self, target: AlState) -> Result<(), EchocatError> {
        let deadline = Instant::now() + self.config.state_transition_timeout;
        loop {
            let states = self.al_states()?;
            if let Some((index, (status, code))) = states
                .iter()
                .enumerate()
                .find(|(_, (status, code))| *code != 0 || *status != Some(target))
                .map(|(index, entry)| (index, *entry))
            {
                if code != 0 {
                    return Err(EchocatError::AlTransition {
                        index,
                        target,
                        status,
                        code,
                    });
                }
            } else {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(EchocatError::AlTimeout {
                    target,
                    timeout: self.config.state_transition_timeout,
                });
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Address, Command, Master, MasterConfig, SleepStrategy};
    use crate::bus::RawBus;
    use crate::reg;
    use crate::sim::EscSim;
    use std::collections::VecDeque;
    use std::io;
    use std::time::{Duration, Instant};

    struct LoopbackBus {
        inner: EscSim,
        echo: VecDeque<Vec<u8>>,
    }

    impl RawBus for LoopbackBus {
        fn send(&mut self, frame: &[u8]) -> io::Result<()> {
            self.echo.push_back(frame.to_vec());
            self.inner.send(frame)
        }

        fn receive(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
            if let Some(frame) = self.echo.pop_front() {
                buf[..frame.len()].copy_from_slice(&frame);
                return Ok(Some(frame.len()));
            }
            self.inner.receive(buf, timeout)
        }

        fn mtu(&self) -> usize {
            self.inner.mtu()
        }
    }

    #[test]
    fn a_reply_that_matches_the_request_byte_for_byte_is_not_taken_for_the_loopback_copy() {
        let cycle = Duration::from_millis(1);
        let bus = LoopbackBus {
            inner: EscSim::nop(1, cycle),
            echo: VecDeque::new(),
        };
        let mut master = Master::open(
            bus,
            MasterConfig {
                cycle,
                dc_static_sync_iterations: 32,
                dc_start_delay: Duration::from_millis(10),
                ..MasterConfig::default()
            },
        )
        .expect("the simulated bus reaches SAFE-OP");

        let unpopulated = Master::<LoopbackBus>::station_address(1);
        let started = Instant::now();
        let (value, wkc) = master
            .read_u16(Command::Fprd, Address::node(unpopulated, reg::AL_STATUS))
            .expect("a working counter of zero is an answer, not a timeout");

        assert_eq!((value, wkc), (0, 0));
        assert!(
            started.elapsed() < Duration::from_millis(50),
            "the reply was discarded as a loopback copy, so the read had to wait out the PDU timeout",
        );
    }

    #[test]
    fn a_deadline_that_has_already_passed_costs_nothing_to_wait_for() {
        for strategy in [
            SleepStrategy::Sleep,
            SleepStrategy::Spin {
                margin: Duration::from_millis(1),
            },
        ] {
            let started = Instant::now();
            strategy.wait_until(
                started
                    .checked_sub(Duration::from_millis(1))
                    .expect("the test clock is past the epoch"),
            );
            assert!(
                started.elapsed() < Duration::from_millis(1),
                "{strategy:?} kept waiting past a deadline it had already missed",
            );
        }
    }

    #[test]
    fn spinning_holds_the_thread_until_the_deadline() {
        let margin = Duration::from_millis(10);
        let wait = Duration::from_millis(1);
        let started = Instant::now();
        SleepStrategy::Spin { margin }.wait_until(started + wait);
        let elapsed = started.elapsed();
        assert!(
            elapsed >= wait,
            "the spin returned after {elapsed:?} instead of {wait:?}, \
             so the frame would land before its DC phase",
        );
    }

    #[test]
    fn the_landing_phase_bias_cancels_a_constant_sleep_overshoot() {
        let cycle = Duration::from_millis(2);
        let mut master = Master::new(
            EscSim::nop(1, cycle),
            MasterConfig {
                cycle,
                ..MasterConfig::default()
            },
        );

        let target = i64::try_from(cycle.as_nanos()).expect("fits") / 2;
        let overshoot = 600_000i64;
        let mut phase = target;
        for _ in 0..64 {
            let bias = i64::try_from(master.phase_bias().as_nanos()).expect("fits");
            phase = target + overshoot - bias;
            master.update_phase_bias(u64::try_from(phase).expect("phase is positive"));
        }

        assert!(
            (phase - target).abs() < 10_000,
            "the landing phase settled at {phase} ns instead of {target} ns; \
             an uncorrected overshoot eats the SYNC0 margin",
        );
    }
}
