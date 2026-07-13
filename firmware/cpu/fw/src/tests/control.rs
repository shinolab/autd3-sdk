use std::vec;
use std::vec::Vec;

use crate::fpga::PWE_TABLE_SIZE;
use crate::params::{
    ADDR_CTL_FLAG, ADDR_DEBUG_VALUE0_0, ADDR_FPGA_STATE, ADDR_VERSION_NUM_MAJOR,
    ADDR_VERSION_NUM_MINOR, ADDR_VERSION_NUM_PATCH, CTL_FLAG_DEBUG_SET, CTL_FLAG_FORCE_FAN,
    CTL_FLAG_GPIO_IN_0, CTL_FLAG_GPIO_IN_1, CTL_FLAG_GPIO_IN_2, CTL_FLAG_GPIO_IN_3,
    NUM_TRANSDUCERS,
};
use crate::proto::{
    CMD_CLEAR, CMD_READ_FPGA_FW_VERSION_MAJOR, CMD_READ_FPGA_FW_VERSION_MINOR,
    CMD_READ_FPGA_FW_VERSION_PATCH, CMD_READ_FPGA_STATE, ERR_INVALID_PAYLOAD, OUTPUT_MASK_WORDS,
    SILENCER_FLAG_STRICT_MODE,
};
use crate::tests::builders::{
    force_fan, gpio_in, gpio_out, output_mask, phase_corr, pwe, set_silencer,
};
use crate::tests::mock::{Frame, Harness};

#[test]
fn force_fan_sets_and_clears_persistent_bit() {
    let mut h = Harness::new();

    h.deliver(&force_fan(0, 1));
    assert_eq!(h.data(), 0);
    assert_eq!(
        h.ctl(ADDR_CTL_FLAG) & CTL_FLAG_FORCE_FAN,
        CTL_FLAG_FORCE_FAN
    );

    h.deliver(&force_fan(1, 0));
    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_CTL_FLAG) & CTL_FLAG_FORCE_FAN, 0);
}

#[test]
fn force_fan_rejects_out_of_range() {
    let mut h = Harness::new();
    h.deliver(&force_fan(0, 2));
    assert_eq!(h.data(), ERR_INVALID_PAYLOAD);
}

#[test]
fn force_fan_survives_subsequent_latch() {
    let mut h = Harness::new();
    h.deliver(&force_fan(0, 1));
    h.deliver(&set_silencer(1, SILENCER_FLAG_STRICT_MODE, 256, 256, 5, 7));
    assert_eq!(
        h.ctl(ADDR_CTL_FLAG) & CTL_FLAG_FORCE_FAN,
        CTL_FLAG_FORCE_FAN
    );
}

#[test]
fn emulate_gpio_in_maps_bits() {
    let mut h = Harness::new();
    h.deliver(&gpio_in(0, 0b1010));
    assert_eq!(h.data(), 0);
    let ctl = h.ctl(ADDR_CTL_FLAG);
    assert_eq!(ctl & CTL_FLAG_GPIO_IN_0, 0);
    assert_eq!(ctl & CTL_FLAG_GPIO_IN_1, CTL_FLAG_GPIO_IN_1);
    assert_eq!(ctl & CTL_FLAG_GPIO_IN_2, 0);
    assert_eq!(ctl & CTL_FLAG_GPIO_IN_3, CTL_FLAG_GPIO_IN_3);
}

#[test]
fn emulate_gpio_in_rejects_out_of_range() {
    let mut h = Harness::new();
    h.deliver(&gpio_in(0, 0x10));
    assert_eq!(h.data(), ERR_INVALID_PAYLOAD);
}

#[test]
fn phase_corr_packs_bytes_into_words() {
    let mut h = Harness::new();
    let phases: Vec<u8> = (0..NUM_TRANSDUCERS).map(|i| (i & 0xFF) as u8).collect();
    h.deliver(&phase_corr(0, &phases));
    assert_eq!(h.data(), 0);
    assert_eq!(
        h.port.phase_corr[0],
        u16::from(phases[0]) | (u16::from(phases[1]) << 8)
    );
    assert_eq!(
        h.port.phase_corr[1],
        u16::from(phases[2]) | (u16::from(phases[3]) << 8)
    );
    assert_eq!(h.port.phase_corr[124], u16::from(phases[248]));
}

#[test]
fn output_mask_writes_words() {
    let mut h = Harness::new();
    let words: Vec<u16> = (0..OUTPUT_MASK_WORDS).map(|i| 0x1000 + i as u16).collect();
    h.deliver(&output_mask(0, &words));
    assert_eq!(h.data(), 0);
    for (i, w) in words.iter().enumerate() {
        assert_eq!(h.port.output_mask[i], *w);
    }
}

#[test]
fn pwe_writes_table() {
    let mut h = Harness::new();
    let table: Vec<u16> = (0..PWE_TABLE_SIZE).map(|i| i as u16).collect();
    h.deliver(&pwe(0, &table));
    assert_eq!(h.data(), 0);
    assert_eq!(h.port.pwe[0], 0);
    assert_eq!(h.port.pwe[1], 1);
    assert_eq!(h.port.pwe[255], 255);
}

#[test]
fn gpio_out_writes_debug_values_and_latches() {
    let mut h = Harness::new();
    let latches_at_boot = h.latch_count(CTL_FLAG_DEBUG_SET);
    let values: Vec<u64> = vec![
        0x0102_0304_0506_0708,
        0x1112_1314_1516_1718,
        0x2122_2324_2526_2728,
        0x3132_3334_3536_3738,
    ];
    h.deliver(&gpio_out(0, &values));
    assert_eq!(h.data(), 0);
    for (v, value) in values.iter().enumerate() {
        for w in 0..4u32 {
            let expect = ((value >> (16 * w)) & 0xFFFF) as u16;
            let addr = ADDR_DEBUG_VALUE0_0 + (v as u16) * 4 + (w as u16);
            assert_eq!(h.ctl(addr), expect);
        }
    }
    assert_eq!(h.latch_count(CTL_FLAG_DEBUG_SET), latches_at_boot + 1);
}

#[test]
fn read_fpga_state_returns_register_byte() {
    let mut h = Harness::new();
    h.set_ctl(ADDR_FPGA_STATE, 0x83);
    h.deliver(&Frame::new(0, CMD_READ_FPGA_STATE));
    assert_eq!(h.data(), 0x83);
}

#[test]
fn read_fpga_fw_version_returns_register_bytes() {
    let mut h = Harness::new();
    h.set_ctl(ADDR_VERSION_NUM_MAJOR, 0x0A);
    h.set_ctl(ADDR_VERSION_NUM_MINOR, 0x0B);
    h.set_ctl(ADDR_VERSION_NUM_PATCH, 0x0C);

    h.deliver(&Frame::new(0, CMD_READ_FPGA_FW_VERSION_MAJOR));
    assert_eq!(h.data(), 0x0A);

    h.deliver(&Frame::new(1, CMD_READ_FPGA_FW_VERSION_MINOR));
    assert_eq!(h.data(), 0x0B);

    h.deliver(&Frame::new(2, CMD_READ_FPGA_FW_VERSION_PATCH));
    assert_eq!(h.data(), 0x0C);
}

#[test]
fn clear_resets_force_fan() {
    let mut h = Harness::new();
    h.deliver(&force_fan(0, 1));
    h.deliver(&Frame::new(1, CMD_CLEAR));
    assert_eq!(h.ctl(ADDR_CTL_FLAG) & CTL_FLAG_FORCE_FAN, 0);
}
