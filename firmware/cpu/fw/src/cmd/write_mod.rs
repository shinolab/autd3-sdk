pub use autd3_cpu_wire::payload::WriteModPayload;

use crate::fpga::{self, MOD_RAM};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let (p, data) = WriteModPayload::parse(payload)?;
    fpga::write_ram(port, &MOD_RAM, p.bank.as_u8(), p.offset.get() / 2, data);
    Ok(())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use std::vec;

    use autd3_cpu_wire::PAYLOAD_BYTES;
    use autd3_cpu_wire::payload::WriteModPayload;

    use crate::fpga::FPGA_PAGE_WORDS;
    use crate::fpga_params::ADDR_MOD_MEM_WR_PAGE;
    use crate::proto::{Error, MOD_BUFFER_SAMPLES};
    use crate::test_utils::builders::{
        assert_fpga_unchanged, fpga_snapshot, invalid_bank, write_mod_buffer,
    };
    use crate::test_utils::mock::Harness;

    #[test]
    fn write_mod_buffer_packs_samples_into_words_per_bank() {
        let mut h = Harness::new();

        h.deliver(&write_mod_buffer(0, 0, 0, &[0x10, 0x20, 0x30, 0x40]));
        assert_eq!(h.status(), Error::None);
        h.deliver(&write_mod_buffer(1, 1, 100, &[0xAA, 0xBB]));
        assert_eq!(h.status(), Error::None);

        assert_eq!(h.mod_word(0, 0), 0x2010);
        assert_eq!(h.mod_word(0, 1), 0x4030);
        assert_eq!(h.mod_word(1, 50), 0xBBAA);
        assert_eq!(h.mod_word(0, 50), 0);
    }

    #[test]
    fn write_mod_buffer_odd_length_pads_high_byte() {
        let mut h = Harness::new();
        h.deliver(&write_mod_buffer(0, 0, 0, &[0xAA]));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.mod_word(0, 0), 0x00AA);
    }

    #[test]
    fn write_mod_buffer_writes_trailing_zero_samples() {
        let mut h = Harness::new();
        h.deliver(&write_mod_buffer(0, 0, 0, &[0x11, 0x22, 0x33, 0x44]));
        h.deliver(&write_mod_buffer(1, 0, 0, &[0x55, 0x00, 0x00, 0x00]));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.mod_word(0, 0), 0x0055);
        assert_eq!(h.mod_word(0, 1), 0x0000);
    }

    #[test]
    fn write_mod_buffer_crosses_page_boundary() {
        let mut h = Harness::new();

        let offset = 2 * FPGA_PAGE_WORDS - 2;
        h.deliver(&write_mod_buffer(0, 0, offset, &[0x01, 0x02, 0x03, 0x04]));
        assert_eq!(h.status(), Error::None);

        let page = FPGA_PAGE_WORDS as usize;
        assert_eq!(h.mod_word(0, page - 1), 0x0201);
        assert_eq!(h.mod_word(0, page), 0x0403);
        assert_eq!(h.ctl(ADDR_MOD_MEM_WR_PAGE), 1);
    }

    #[test]
    fn write_mod_buffer_accepts_chunked_writes_up_to_capacity() {
        let mut h = Harness::new();

        let mut seq: u8 = 0;
        let mut written: u32 = 0;
        let mut last = 0u8;
        while written < MOD_BUFFER_SAMPLES {
            let len = u32::try_from(PAYLOAD_BYTES - size_of::<WriteModPayload>())
                .unwrap()
                .min(MOD_BUFFER_SAMPLES - written);
            last = (written >> 8) as u8;
            let chunk = vec![last; len as usize];
            h.deliver(&write_mod_buffer(seq, 0, written, &chunk));
            assert_eq!(h.status(), Error::None);
            seq = seq.wrapping_add(1);
            written += len;
        }
        let expected = (u16::from(last) << 8) | u16::from(last);
        assert_eq!(h.mod_word(0, MOD_BUFFER_SAMPLES as usize / 2 - 1), expected);
    }

    #[test]
    fn write_mod_buffer_empty_data_is_no_op_success() {
        let mut h = Harness::new();
        h.deliver(&write_mod_buffer(0, 0, 0, &[]));
        assert_eq!(h.ack(), 0);
        assert_eq!(h.status(), Error::None);
    }

    #[test]
    fn write_mod_buffer_rejects_invalid_payloads() {
        let mut h = Harness::new();

        h.deliver(&write_mod_buffer(0, invalid_bank(), 0, &[0x01]));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&write_mod_buffer(1, 0, 1, &[0x01, 0x02]));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&write_mod_buffer(
            2,
            0,
            MOD_BUFFER_SAMPLES - 2,
            &[0x01, 0x02, 0x03],
        ));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_eq!(h.mod_word(0, MOD_BUFFER_SAMPLES as usize / 2 - 1), 0);

        let before = fpga_snapshot(&h);
        h.deliver(&write_mod_buffer(3, 0, u32::MAX - 1, &[0x01, 0x02]));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_fpga_unchanged(&before, &h);
    }
}
