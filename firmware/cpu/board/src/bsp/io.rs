#[cfg(feature = "isr-probe")]
use crate::regs::PORTA_PODR;
use crate::regs::{
    PORT5_PDR, PORTA_PDR, PORTA_PMR, PORTF_PDR, PORTN_PDR, PORTN_PODR, modify8, pdr_set,
};

const PDR_OUTPUT: u16 = 3;
const PDR_INPUT: u16 = 0;

const LED1_PIN: u8 = 6;
const LED2_PIN: u8 = 7;

#[cfg(feature = "isr-probe")]
const ISR_PROBE_PIN: u8 = 5;

#[cfg(feature = "isr-probe")]
pub(crate) fn isr_probe_high() {
    modify8(PORTA_PODR, |v| v | (1 << ISR_PROBE_PIN));
}

#[cfg(feature = "isr-probe")]
pub(crate) fn isr_probe_low() {
    modify8(PORTA_PODR, |v| v & !(1 << ISR_PROBE_PIN));
}

pub(crate) fn init() {
    super::pfs_write_enable();

    pdr_set(PORT5_PDR, 6, PDR_OUTPUT);

    pdr_set(PORTA_PDR, 4, PDR_INPUT);
    modify8(PORTA_PMR, |v| v & !(1 << 4));
    pdr_set(PORTA_PDR, 5, PDR_OUTPUT);
    modify8(PORTA_PMR, |v| v & !(1 << 5));
    pdr_set(PORTA_PDR, 6, PDR_INPUT);
    modify8(PORTA_PMR, |v| v & !(1 << 6));
    pdr_set(PORTA_PDR, 7, PDR_OUTPUT);
    modify8(PORTA_PMR, |v| v & !(1 << 7));

    #[cfg(feature = "isr-probe")]
    modify8(PORTA_PODR, |v| v & !(1 << ISR_PROBE_PIN));

    pdr_set(PORTF_PDR, 7, PDR_OUTPUT);

    modify8(PORTN_PODR, |v| v | (1 << LED1_PIN));
    modify8(PORTN_PODR, |v| v | (1 << LED2_PIN));
    pdr_set(PORTN_PDR, u16::from(LED1_PIN), PDR_OUTPUT);
    pdr_set(PORTN_PDR, u16::from(LED2_PIN), PDR_OUTPUT);

    super::pfs_write_disable();
}
