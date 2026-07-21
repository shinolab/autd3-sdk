use crate::error::Error;
use crate::params::NUM_BANKS;
use crate::value::{LoopBehavior, TransitionMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BankLoop {
    Infinite,
    Finite,
}

impl BankLoop {
    #[must_use]
    pub const fn of(loop_behavior: LoopBehavior) -> Self {
        match loop_behavior {
            LoopBehavior::Infinite => BankLoop::Infinite,
            LoopBehavior::Finite(_) => BankLoop::Finite,
        }
    }

    #[must_use]
    pub const fn accepts(self, mode: TransitionMode) -> bool {
        match self {
            BankLoop::Infinite => {
                matches!(mode, TransitionMode::Immediate | TransitionMode::Ext)
            }
            BankLoop::Finite => matches!(
                mode,
                TransitionMode::SyncIdx
                    | TransitionMode::SysTime {
                        time: _,
                        margin: None
                    }
                    | TransitionMode::Gpio(_)
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionGuardState {
    pub mod_loop: [BankLoop; NUM_BANKS],
    pub pattern_loop: [BankLoop; NUM_BANKS],
}

impl TransitionGuardState {
    #[must_use]
    pub fn boot_default() -> Self {
        Self {
            mod_loop: [BankLoop::Infinite; NUM_BANKS],
            pattern_loop: [BankLoop::Infinite; NUM_BANKS],
        }
    }

    pub fn note_mod_loop(&mut self, bank: u8, loop_behavior: LoopBehavior) {
        self.mod_loop[usize::from(bank)] = BankLoop::of(loop_behavior);
    }

    pub fn note_pattern_loop(&mut self, bank: u8, loop_behavior: LoopBehavior) {
        self.pattern_loop[usize::from(bank)] = BankLoop::of(loop_behavior);
    }

    pub fn check_mod_bank(
        &self,
        device: usize,
        bank: u8,
        mode: TransitionMode,
    ) -> Result<(), Error> {
        check(device, self.mod_loop[usize::from(bank)], mode)
    }

    pub fn check_pattern_bank(
        &self,
        device: usize,
        bank: u8,
        mode: TransitionMode,
    ) -> Result<(), Error> {
        check(device, self.pattern_loop[usize::from(bank)], mode)
    }
}

fn check(device: usize, bank_loop: BankLoop, mode: TransitionMode) -> Result<(), Error> {
    if bank_loop.accepts(mode) {
        Ok(())
    } else {
        Err(Error::TransitionConstraint {
            device,
            transition_mode: mode,
            bank_loop,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{DcSysTime, GpioIn};
    use core::num::NonZeroU16;

    const FINITE: LoopBehavior = LoopBehavior::ONCE;
    const INFINITE: LoopBehavior = LoopBehavior::Infinite;

    #[test]
    fn boot_default_is_all_infinite() {
        let g = TransitionGuardState::boot_default();
        assert_eq!(g.mod_loop, [BankLoop::Infinite; NUM_BANKS]);
        assert_eq!(g.pattern_loop, [BankLoop::Infinite; NUM_BANKS]);
    }

    #[test]
    fn infinite_accepts_only_immediate_and_ext() {
        assert!(BankLoop::Infinite.accepts(TransitionMode::Immediate));
        assert!(BankLoop::Infinite.accepts(TransitionMode::Ext));
        assert!(!BankLoop::Infinite.accepts(TransitionMode::SyncIdx));
        assert!(!BankLoop::Infinite.accepts(TransitionMode::SysTime {
            time: DcSysTime::from_nanos(0),
            margin: None
        }));
        assert!(!BankLoop::Infinite.accepts(TransitionMode::Gpio(GpioIn::I0)));
    }

    #[test]
    fn finite_accepts_only_timed_modes() {
        assert!(BankLoop::Finite.accepts(TransitionMode::SyncIdx));
        assert!(BankLoop::Finite.accepts(TransitionMode::SysTime {
            time: DcSysTime::from_nanos(0),
            margin: None
        }));
        assert!(BankLoop::Finite.accepts(TransitionMode::Gpio(GpioIn::I0)));
        assert!(!BankLoop::Finite.accepts(TransitionMode::Immediate));
        assert!(!BankLoop::Finite.accepts(TransitionMode::Ext));
    }

    #[test]
    fn check_mod_bank_uses_target_bank_loop() {
        let mut g = TransitionGuardState::boot_default();
        g.note_mod_loop(1, LoopBehavior::Finite(NonZeroU16::new(5).unwrap()));

        assert!(g.check_mod_bank(0, 0, TransitionMode::Immediate).is_ok());
        assert!(matches!(
            g.check_mod_bank(2, 1, TransitionMode::Immediate),
            Err(Error::TransitionConstraint {
                device: 2,
                transition_mode: TransitionMode::Immediate,
                bank_loop: BankLoop::Finite,
            })
        ));
        assert!(g.check_mod_bank(0, 1, TransitionMode::SyncIdx).is_ok());
    }

    #[test]
    fn check_pattern_bank_uses_target_bank_loop() {
        let mut g = TransitionGuardState::boot_default();
        g.note_pattern_loop(0, FINITE);
        g.note_pattern_loop(1, INFINITE);

        assert!(
            g.check_pattern_bank(0, 0, TransitionMode::Gpio(GpioIn::I1))
                .is_ok()
        );
        assert!(g.check_pattern_bank(0, 0, TransitionMode::Ext).is_err());
        assert!(g.check_pattern_bank(0, 1, TransitionMode::Ext).is_ok());
        assert!(g.check_pattern_bank(0, 1, TransitionMode::SyncIdx).is_err());
    }
}
