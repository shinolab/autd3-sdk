use std::time::{Duration, Instant};

use super::state::BusState;
use super::{CycleReport, Master, MasterConfig};
use crate::bus::RawBus;
use crate::error::EchocatError;
use crate::reg::{self, AlState};
use crate::wire::{Address, Command};

const OP_PRIMING_CYCLES: usize = 8;
const PHASE_WARN_INTERVAL: Duration = Duration::from_secs(5);

impl<B: RawBus> Master<B> {
    pub fn open(bus: B, config: MasterConfig) -> Result<Self, EchocatError> {
        let mut master = Self::new(bus, config);
        master.bring_up()?;
        Ok(master)
    }

    pub fn bring_up(&mut self) -> Result<(), EchocatError> {
        let devices = self.enumerate()?;
        self.state = BusState::new(devices);
        tracing::info!(devices, "enumerated the EtherCAT bus");

        self.acknowledge_errors_and_request_init()?;
        self.verify_identity()?;
        self.configure_mailbox_sync_managers()?;
        self.request_state(AlState::PreOp)?;

        self.configure_process_data()?;
        self.init_dc()?;
        self.plan_cycle();
        self.request_state(AlState::SafeOp)?;

        tracing::info!(
            devices,
            "the EtherCAT bus is in SAFE-OP; OP is entered on the first cycle"
        );
        Ok(())
    }

    fn reach_op(&mut self, tx: &[u8], rx: &mut [u8]) -> Result<(), EchocatError> {
        for _ in 0..OP_PRIMING_CYCLES {
            self.paced_cycle(tx, rx)?;
        }

        let expected = u16::try_from(self.devices).expect("device count fits in u16");
        let wkc = self.write_u16(
            Command::Bwr,
            Address::broadcast(reg::AL_CONTROL),
            u16::from(AlState::Op.code()),
        )?;
        Self::expect_wkc(wkc, expected)?;

        let deadline = Instant::now() + self.config.state_transition_timeout;
        loop {
            self.paced_cycle(tx, rx)?;
            let states = self.al_states()?;
            if let Some((index, (status, code))) = states
                .iter()
                .enumerate()
                .find(|(_, (_, code))| *code != 0)
                .map(|(index, entry)| (index, *entry))
            {
                return Err(EchocatError::AlTransition {
                    index,
                    target: AlState::Op,
                    status,
                    code,
                });
            }
            if states
                .iter()
                .all(|(status, _)| *status == Some(AlState::Op))
            {
                tracing::info!(devices = self.devices, "the EtherCAT bus is in OP");
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(EchocatError::AlTimeout {
                    target: AlState::Op,
                    timeout: self.config.state_transition_timeout,
                });
            }
        }
    }

    #[must_use]
    pub fn is_op(&self) -> bool {
        self.op_entered
    }

    pub fn cycle(&mut self, tx: &[u8], rx: &mut [u8]) -> Result<CycleReport, EchocatError> {
        if !self.op_entered {
            self.reach_op(tx, rx)?;
            self.op_entered = true;
        }
        self.paced_cycle(tx, rx)
    }

    fn paced_cycle(&mut self, tx: &[u8], rx: &mut [u8]) -> Result<CycleReport, EchocatError> {
        if let Some(deadline) = self.next_at {
            self.config.sleep_strategy.wait_until(deadline);
        }
        let anchor = Instant::now();
        if let Some(previous) = self.last_cycle_at {
            let gap = anchor.duration_since(previous);
            if gap > self.config.cycle * 2 {
                tracing::warn!(
                    ?gap,
                    cycle = ?self.config.cycle,
                    missed_sync0 = gap.as_nanos() / self.config.cycle.as_nanos().max(1),
                    "process data stopped for longer than a SYNC0 period; \
                     the subdevice may drop to SAFE-OP with a synchronization error",
                );
            }
        }
        self.last_cycle_at = Some(anchor);
        let report = self.exchange(tx, rx)?;
        self.report_landing_phase(report.dc_system_time, anchor);
        self.update_phase_bias(report.dc_system_time);
        self.next_at = (report.dc_system_time != 0)
            .then(|| anchor + report.next_cycle_wait.saturating_sub(self.phase_bias()));
        Ok(report)
    }

    fn report_landing_phase(&mut self, dc_system_time: u64, at: Instant) {
        if dc_system_time == 0 {
            return;
        }
        let cycle_ns = u64::try_from(self.config.cycle.as_nanos()).expect("cycle fits in u64");
        if cycle_ns == 0 {
            return;
        }
        let phase = dc_system_time % cycle_ns;
        let target = cycle_ns / 2;
        tracing::trace!(phase_ns = phase, target_ns = target, "frame landing phase");

        let deviation = phase.abs_diff(target);
        if deviation <= cycle_ns / 4 {
            return;
        }
        self.phase_excursions += 1;
        self.worst_phase_ns = self.worst_phase_ns.max(deviation);
        if self
            .last_phase_warn_at
            .is_some_and(|last| at.duration_since(last) < PHASE_WARN_INTERVAL)
        {
            return;
        }
        self.last_phase_warn_at = Some(at);
        tracing::debug!(
            excursions = self.phase_excursions,
            worst_deviation_ns = self.worst_phase_ns,
            target_ns = target,
            cycle_ns,
            "frames are landing away from the middle of the SYNC0 period",
        );
        self.phase_excursions = 0;
        self.worst_phase_ns = 0;
    }

    pub fn recover_op(&mut self) -> Result<(), EchocatError> {
        let mut recovered = false;
        for index in 0..self.devices {
            let Some(raw) = self.state.al_status(index) else {
                continue;
            };
            if raw == AlState::Op.code() {
                continue;
            }
            let node = Self::station_address(index);
            let errored = raw & AlState::ERROR_FLAG != 0;
            let control = if errored {
                u16::from(raw)
            } else {
                u16::from(AlState::Op.code())
            };
            if errored {
                let (code, _) =
                    self.read_u16(Command::Fprd, Address::node(node, reg::AL_STATUS_CODE))?;
                tracing::warn!(
                    device = index,
                    state = ?AlState::from_code(raw),
                    al_status_code = format_args!("{code:#06x}"),
                    reason = reg::al_status_code_str(code),
                    al_control = format_args!("{control:#06x}"),
                    "device left OP with an error; acknowledging it",
                );
            } else {
                tracing::debug!(
                    device = index,
                    state = ?AlState::from_code(raw),
                    al_control = format_args!("{control:#06x}"),
                    "device is below OP; requesting OP again",
                );
            }
            self.write_u16(Command::Fpwr, Address::node(node, reg::AL_CONTROL), control)?;
            recovered = true;
        }
        if recovered {
            self.state.record_recovery();
        }
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), EchocatError> {
        self.write_u16(
            Command::Bwr,
            Address::broadcast(reg::AL_CONTROL),
            u16::from(AlState::Init.code()),
        )?;
        self.next_at = None;
        Ok(())
    }

    #[must_use]
    pub fn cycle_time(&self) -> Duration {
        self.config.cycle
    }
}
