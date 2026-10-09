pub use autd3_cpu_wire::payload::WriteFociPayload;

use crate::fpga::{self, EMISSION_RAM};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let (p, data) = WriteFociPayload::parse(payload)?;
    fpga::write_ram(port, &EMISSION_RAM, p.bank.as_u8(), p.offset.get(), data);
    Ok(())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use autd3_cpu_wire::PatternBank;
    use zerocopy::little_endian::U32;

    use super::WriteFociPayload;
    use crate::fpga::FPGA_PAGE_WORDS;
    use crate::fpga_params::{ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE};
    use crate::proto::{Cmd, EMISSION_RAM_WORDS, Error};
    use crate::test_utils::builders::{
        assert_fpga_unchanged, fpga_snapshot, invalid_bank, write_foci_buffer,
    };
    use crate::test_utils::mock::{Frame, Harness};

    #[test]
    fn write_foci_buffer_writes_words_at_offset_per_bank() {
        let mut h = Harness::new();

        h.deliver(&write_foci_buffer(0, 0, 0, &[0x1234, 0x5678]));
        assert_eq!(h.status(), Error::None);
        h.deliver(&write_foci_buffer(1, 1, 300, &[0xAABB]));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.expected_seq(), 2);

        assert_eq!(h.emission_word(0, 0), 0x1234);
        assert_eq!(h.emission_word(0, 1), 0x5678);
        assert_eq!(h.emission_word(1, 300), 0xAABB);
        assert_eq!(h.emission_word(0, 300), 0);
    }

    #[test]
    fn write_foci_buffer_crosses_page_boundary() {
        let mut h = Harness::new();

        let page = FPGA_PAGE_WORDS as usize;
        h.deliver(&write_foci_buffer(
            0,
            0,
            FPGA_PAGE_WORDS - 2,
            &[0x0001, 0x0002, 0x0003, 0x0004],
        ));
        assert_eq!(h.status(), Error::None);

        assert_eq!(h.emission_word(0, page - 2), 0x0001);
        assert_eq!(h.emission_word(0, page - 1), 0x0002);
        assert_eq!(h.emission_word(0, page), 0x0003);
        assert_eq!(h.emission_word(0, page + 1), 0x0004);
        assert_eq!(h.ctl(ADDR_PATTERN_MEM_WR_PAGE), 1);
    }

    #[test]
    fn write_foci_buffer_empty_data_is_no_op_success() {
        let mut h = Harness::new();
        h.deliver(&write_foci_buffer(0, 0, 0, &[]));
        assert_eq!(h.ack(), 0);
        assert_eq!(h.status(), Error::None);
    }

    #[test]
    fn write_foci_buffer_empty_data_at_ram_end_does_not_switch_bank_or_page() {
        const UNTOUCHED: u16 = 0xBEEF;
        let mut h = Harness::new();
        h.set_ctl(ADDR_PATTERN_MEM_WR_BANK, UNTOUCHED);
        h.set_ctl(ADDR_PATTERN_MEM_WR_PAGE, UNTOUCHED);

        h.deliver(&write_foci_buffer(0, 1, EMISSION_RAM_WORDS, &[]));

        assert_eq!(h.ack(), 0);
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_PATTERN_MEM_WR_BANK), UNTOUCHED);
        assert_eq!(h.ctl(ADDR_PATTERN_MEM_WR_PAGE), UNTOUCHED);
    }

    #[test]
    fn write_foci_buffer_rejects_invalid_payloads() {
        let mut h = Harness::new();

        h.deliver(&write_foci_buffer(0, invalid_bank(), 0, &[0x0001]));
        assert_eq!(h.status(), Error::InvalidPayload);

        let odd = WriteFociPayload {
            bank: PatternBank::B0,
            reserved: 0,
            offset: U32::new(0),
        };
        h.deliver(&Frame::from_parts(
            1,
            Cmd::WriteFociBuffer,
            &odd,
            &[0x01, 0x02, 0x03],
        ));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_eq!(h.emission_word(0, 0), 0);

        h.deliver(&write_foci_buffer(
            3,
            0,
            EMISSION_RAM_WORDS - 1,
            &[0x0001, 0x0002],
        ));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_eq!(h.emission_word(0, EMISSION_RAM_WORDS as usize - 1), 0);

        let before = fpga_snapshot(&h);
        h.deliver(&write_foci_buffer(4, 0, u32::MAX, &[0x0001]));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_fpga_unchanged(&before, &h);
    }
}
