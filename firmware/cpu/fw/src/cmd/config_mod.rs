use core::mem::offset_of;

use zerocopy::little_endian::{U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::app::Cpu;
use crate::fpga;
use crate::params::{
    ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0, ADDR_MOD_REP0, BRAM_SELECT_CONTROLLER, CTL_FLAG_MOD_SET,
    NUM_BANKS,
};
use crate::port::Port;
use crate::proto::{Error, MOD_BUFFER_SAMPLES};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ConfigModPayload {
    pub bank: u8,
    _reserved: u8,
    pub divider: U16,
    pub size: U32,
    pub rep: U16,
}

const _: () = assert!(offset_of!(ConfigModPayload, bank) == 0);
const _: () = assert!(offset_of!(ConfigModPayload, divider) == 2);
const _: () = assert!(offset_of!(ConfigModPayload, size) == 4);
const _: () = assert!(offset_of!(ConfigModPayload, rep) == 8);

impl Cpu {
    pub(crate) fn config_mod<P: Port>(&self, port: &mut P, payload: &[u8]) -> Result<(), Error> {
        let Ok((p, _)) = ConfigModPayload::ref_from_prefix(payload) else {
            return Err(Error::InvalidPayload);
        };
        let bank = p.bank;
        let divider = p.divider.get();
        let size = p.size.get();
        let rep = p.rep.get();

        if usize::from(bank) >= NUM_BANKS || divider == 0 || size == 0 || size > MOD_BUFFER_SAMPLES
        {
            return Err(Error::InvalidPayload);
        }
        if self.silencer.violates_mod_div(divider) {
            return Err(Error::InvalidSilencerSetting);
        }

        let bank_offset = u16::from(bank);
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_MOD_CYCLE0 + bank_offset,
            (size - 1) as u16,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_MOD_FREQ_DIV0 + bank_offset,
            divider,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_MOD_REP0 + bank_offset,
            rep,
        );
        self.silencer.note_mod_div(bank, divider);
        self.set_and_wait_update(port, CTL_FLAG_MOD_SET)
    }
}
