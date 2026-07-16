use core::mem::offset_of;

use zerocopy::little_endian::{U32, U64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::app::Cpu;
use crate::fpga::{self, TransitionMode, sys_time_margin_ns, transition_mode_violates_loop};
use crate::params::{
    ADDR_PATTERN_REP0, ADDR_PATTERN_REQ_RD_BANK, ADDR_PATTERN_TRANSITION_MODE,
    ADDR_PATTERN_TRANSITION_VALUE_0, BRAM_SELECT_CONTROLLER, CTL_FLAG_PATTERN_SET, NUM_BANKS,
};
use crate::port::Port;
use crate::proto::Error;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ChangePatternBankPayload {
    pub bank: u8,
    pub transition_mode: u8,
    pub transition_value: U64,
    pub margin_ns: U32,
}

const _: () = assert!(offset_of!(ChangePatternBankPayload, bank) == 0);
const _: () = assert!(offset_of!(ChangePatternBankPayload, transition_mode) == 1);
const _: () = assert!(offset_of!(ChangePatternBankPayload, transition_value) == 2);
const _: () = assert!(offset_of!(ChangePatternBankPayload, margin_ns) == 10);

impl Cpu {
    pub(crate) fn change_pattern_bank<P: Port>(
        &self,
        port: &mut P,
        payload: &[u8],
    ) -> Result<(), Error> {
        let Ok((p, _)) = ChangePatternBankPayload::ref_from_prefix(payload) else {
            return Err(Error::InvalidPayload);
        };
        let bank = p.bank;
        let transition_value = p.transition_value.get();
        let margin_ns = sys_time_margin_ns(p.margin_ns.get());

        if usize::from(bank) >= NUM_BANKS {
            return Err(Error::InvalidPayload);
        }
        if self.silencer.violates_pattern_bank(bank) {
            return Err(Error::InvalidSilencerSetting);
        }
        let rep = fpga::read(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_REP0 + u16::from(bank),
        );
        let Some(transition_mode) = TransitionMode::from_u8(p.transition_mode) else {
            return Err(Error::InvalidTransitionMode);
        };
        if transition_mode_violates_loop(rep, transition_mode) {
            return Err(Error::InvalidTransitionMode);
        }
        if transition_mode == TransitionMode::SysTime
            && transition_value < port.dc_sys_time() + margin_ns
        {
            return Err(Error::MissTransitionTime);
        }

        fpga::write_change_bank(
            port,
            ADDR_PATTERN_REQ_RD_BANK,
            ADDR_PATTERN_TRANSITION_MODE,
            ADDR_PATTERN_TRANSITION_VALUE_0,
            bank,
            transition_mode,
            transition_value,
        );
        self.silencer.note_pattern_bank(bank);
        self.set_and_wait_update(port, CTL_FLAG_PATTERN_SET)
    }
}
