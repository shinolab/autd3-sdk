use std::time::{Duration, Instant};

use super::cyclic::Recovery;
use super::state::BusState;
use super::{CycleReport, Master, MasterConfig, PHASE_TOLERANCE_DIVISOR, next_cycle_wait};
use crate::bus::RawBus;
use crate::error::EchocatError;
use crate::reg::{self, AlState};
use crate::wire::{Address, Command};

const OP_PRIMING_CYCLES: usize = 8;

#[must_use]
pub fn phase_deviation_ns(phase_ns: u64, target_ns: u64, cycle_ns: u64) -> u64 {
    if cycle_ns == 0 {
        return 0;
    }
    let forward = (phase_ns + cycle_ns - target_ns % cycle_ns) % cycle_ns;
    forward.min(cycle_ns - forward)
}

#[must_use]
pub fn phase_tolerance_ns(cycle_ns: u64) -> u64 {
    cycle_ns / PHASE_TOLERANCE_DIVISOR
}
const PHASE_WARN_INTERVAL: Duration = Duration::from_secs(5);
const RECOVERY_LOG_INTERVAL: Duration = Duration::from_secs(1);

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

    pub fn wait_next_cycle(&mut self) {
        if let Some(deadline) = self.next_at {
            self.config.sleep_strategy.wait_until(deadline);
        }
    }

    fn paced_cycle(&mut self, tx: &[u8], rx: &mut [u8]) -> Result<CycleReport, EchocatError> {
        let entered = Instant::now();
        self.wait_next_cycle();
        let anchor = Instant::now();
        if let Some(previous) = self.last_cycle_at {
            let gap = anchor.duration_since(previous);
            if gap > self.config.cycle * 2 {
                tracing::warn!(
                    ?gap,
                    cycle = ?self.config.cycle,
                    missed_sync0 = gap.as_nanos() / self.config.cycle.as_nanos().max(1),
                    prev_exchange = ?self.last_exchange,
                    slept = ?anchor.duration_since(entered),
                    overshoot = ?self
                        .next_at
                        .map(|deadline| anchor.saturating_duration_since(deadline)),
                    "process data stopped for longer than a SYNC0 period; \
                     the subdevice may drop to SAFE-OP with a synchronization error",
                );
            }
        }
        self.last_cycle_at = Some(anchor);
        let exchange_at = Instant::now();
        let report = self.exchange(tx, rx)?;
        self.last_exchange = exchange_at.elapsed();
        let exchange_ns = u64::try_from(self.last_exchange.as_nanos()).unwrap_or(u64::MAX);
        self.stats.record_exchange(exchange_ns);
        self.observe_exchange(exchange_ns);
        let wait = next_cycle_wait(
            report.dc_system_time,
            self.config.cycle,
            self.landing_target_ns(),
        );
        self.report_landing_phase(report.dc_system_time, anchor);
        self.update_phase_bias(report.dc_system_time);
        self.next_at =
            (report.dc_system_time != 0).then(|| anchor + wait.saturating_sub(self.phase_bias()));
        Ok(report)
    }

    fn report_landing_phase(&mut self, dc_system_time: u64, at: Instant) {
        if dc_system_time == 0 {
            return;
        }
        let cycle_ns = self.cycle_ns();
        if cycle_ns == 0 {
            return;
        }
        let phase = dc_system_time % cycle_ns;
        let target = self.landing_target_ns();
        tracing::trace!(phase_ns = phase, target_ns = target, "frame landing phase");

        let deviation = phase_deviation_ns(phase, target, cycle_ns);
        if deviation <= phase_tolerance_ns(cycle_ns) {
            return;
        }
        self.phase_excursions += 1;
        self.worst_phase_ns = self.worst_phase_ns.max(deviation);
        self.stats.record_phase_excursion(deviation);
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
            exchange_ns = self.exchange_estimate_ns,
            "frames are landing away from their target phase in the SYNC0 period",
        );
        self.phase_excursions = 0;
        self.worst_phase_ns = 0;
    }

    pub(crate) fn plan_recovery(&mut self) -> Option<Recovery> {
        if !self.op_entered || self.devices == 0 {
            return None;
        }
        let device = (self.rotation + self.devices - 1) % self.devices;
        let raw = self.state.al_status(device)?;
        if raw == AlState::Op.code() {
            return None;
        }
        let errored = raw & AlState::ERROR_FLAG != 0;
        Some(Recovery {
            device,
            status: raw,
            code: self.state.al_status_code(device).unwrap_or(0),
            control: if errored {
                u16::from(raw)
            } else {
                u16::from(AlState::Op.code())
            },
        })
    }

    pub(crate) fn account_for_recovery(&mut self, recovery: Recovery, delivered: bool) {
        if !delivered {
            return;
        }
        self.state.record_recovery();
        self.recovery_attempts = self.recovery_attempts.saturating_add(1);
        if self
            .last_recovery_log_at
            .is_some_and(|last| last.elapsed() < RECOVERY_LOG_INTERVAL)
        {
            return;
        }
        self.last_recovery_log_at = Some(Instant::now());
        let attempts = std::mem::take(&mut self.recovery_attempts);
        if recovery.status & AlState::ERROR_FLAG != 0 {
            tracing::warn!(
                device = recovery.device,
                state = ?AlState::from_code(recovery.status),
                al_status_code = format_args!("{:#06x}", recovery.code),
                reason = reg::al_status_code_str(recovery.code),
                al_control = format_args!("{:#06x}", recovery.control),
                attempts,
                "device left OP with an error; acknowledging it",
            );
        } else {
            tracing::debug!(
                device = recovery.device,
                state = ?AlState::from_code(recovery.status),
                al_control = format_args!("{:#06x}", recovery.control),
                attempts,
                "device is below OP; requesting OP again",
            );
        }
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

    #[must_use]
    pub fn exchange_estimate(&self) -> Duration {
        Duration::from_nanos(self.exchange_estimate_ns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CYCLE: u64 = 1_000_000;

    #[test]
    fn the_deviation_is_the_shorter_way_round_the_period() {
        assert_eq!(phase_deviation_ns(500_000, 500_000, CYCLE), 0);
        assert_eq!(phase_deviation_ns(600_000, 500_000, CYCLE), 100_000);
        assert_eq!(phase_deviation_ns(400_000, 500_000, CYCLE), 100_000);
        assert_eq!(
            phase_deviation_ns(10_000, 990_000, CYCLE),
            20_000,
            "a deviation across the SYNC0 edge is the short way round, not 980 us",
        );
        assert_eq!(
            phase_deviation_ns(0, 500_000, CYCLE),
            500_000,
            "the far side"
        );
    }

    #[test]
    fn the_tolerance_does_not_depend_on_the_requested_landing_phase() {
        let tolerance = phase_tolerance_ns(CYCLE);
        assert_eq!(tolerance, CYCLE / 16);
        for target in [10_000u64, 250_000, 500_000, 750_000, 990_000] {
            let just_outside =
                phase_deviation_ns((target + tolerance + 1_000) % CYCLE, target, CYCLE);
            assert!(
                just_outside > tolerance,
                "a {}, us miss went unreported at a {} us target",
                just_outside / 1_000,
                target / 1_000,
            );
        }
    }

    #[test]
    fn the_tolerance_scales_with_the_period() {
        assert_eq!(
            phase_tolerance_ns(2_000_000),
            2 * phase_tolerance_ns(1_000_000)
        );
    }
}
