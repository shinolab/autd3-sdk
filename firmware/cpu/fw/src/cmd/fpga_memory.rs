use autd3_cpu_wire::payload::{ReadFpgaMemoryPayload, WriteFpgaMemoryPayload};

use crate::fpga;
use crate::port::Port;
use crate::proto::{Error, REPLY_DATA_BYTES_MAX, ReplyData};

pub(crate) fn write<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let (p, data) = WriteFpgaMemoryPayload::parse(payload)?;
    port.memory_barrier();
    for (addr, word) in (p.addr.get()..=u16::MAX).zip(fpga::le_words(data, 0)) {
        port.fpga_write(addr, word);
        port.memory_barrier();
    }
    Ok(())
}

pub(crate) fn read<P: Port>(port: &mut P, payload: &[u8]) -> Result<ReplyData, Error> {
    let p = ReadFpgaMemoryPayload::parse(payload)?;
    let mut data = [0; REPLY_DATA_BYTES_MAX];
    let len = usize::from(p.count.get()) * size_of::<u16>();
    for (addr, word) in (p.addr.get()..=u16::MAX).zip(data[..len].as_chunks_mut::<2>().0) {
        *word = port.fpga_read(addr).to_le_bytes();
    }
    Ok(ReplyData::from_slice(&data[..len]))
}

#[cfg(all(test, not(loom)))]
mod tests {
    use zerocopy::little_endian::U16;

    use autd3_cpu_wire::payload::{ReadFpgaMemoryPayload, WriteFpgaMemoryPayload};

    use crate::fpga_params::{
        ADDR_CTL_FLAG, ADDR_DEBUG_VALUE0_0, ADDR_MOD_MEM_WR_BANK, ADDR_MOD_MEM_WR_PAGE, BramSelect,
        CtlFlags,
    };
    use crate::proto::{Cmd, Error};
    use crate::test_utils::builders::{assert_fpga_unchanged, fpga_snapshot};
    use crate::test_utils::mock::{Frame, Harness};

    fn bus_addr(select: BramSelect, addr: u16) -> u16 {
        (u16::from(select.as_u8()) << 8) | addr
    }

    fn write(addr: u16, words: &[u16]) -> Frame {
        let data: std::vec::Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        Frame::from_parts(
            Cmd::WriteFpgaMemory,
            &WriteFpgaMemoryPayload { addr: addr.into() },
            &data,
        )
    }

    fn read(addr: u16, count: u16) -> Frame {
        Frame::from_payload(
            Cmd::ReadFpgaMemory,
            &ReadFpgaMemoryPayload {
                addr: addr.into(),
                count: count.into(),
            },
        )
    }

    fn reply_words(h: &Harness) -> std::vec::Vec<u16> {
        h.reply_data()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|word| u16::from_le_bytes(*word))
            .collect()
    }

    #[test]
    fn written_controller_words_read_back() {
        let mut h = Harness::new();
        h.deliver(&write(ADDR_DEBUG_VALUE0_0, &[0x1234, 0xABCD, 0x0F0F]));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.reply_data(), [0u8; 0]);

        h.deliver(&read(ADDR_DEBUG_VALUE0_0, 3));
        assert_eq!(h.status(), Error::None);
        assert_eq!(reply_words(&h), [0x1234, 0xABCD, 0x0F0F]);
    }

    #[test]
    fn a_write_reaches_every_bram_select_through_the_raw_bus_address() {
        let mut h = Harness::new();
        h.deliver(&write(bus_addr(BramSelect::OutputMask, 3), &[0x00FF]));
        h.deliver(&write(bus_addr(BramSelect::PweTable, 7), &[0x0155]));
        h.deliver(&write(ADDR_MOD_MEM_WR_BANK, &[1, 1]));
        h.deliver(&write(bus_addr(BramSelect::Mod, 5), &[0xBEEF, 0xCAFE]));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.output_mask[3], 0x00FF);
        assert_eq!(h.port.pwe[7], 0x0155);
        assert_eq!(h.port.ctl[ADDR_MOD_MEM_WR_PAGE as usize], 1);
        assert_eq!(h.mod_word(1, (1 << 14) | 5), 0xBEEF);
        assert_eq!(h.mod_word(1, (1 << 14) | 6), 0xCAFE);
    }

    #[test]
    fn every_written_word_sits_between_barriers() {
        let mut h = Harness::new();
        h.port.barriers = 0;
        h.deliver(&write(ADDR_MOD_MEM_WR_BANK, &[1, 0]));
        assert_eq!(h.port.barriers, 3);
    }

    #[test]
    fn a_read_does_not_write_the_fpga() {
        let mut h = Harness::new();
        let before = fpga_snapshot(&h);
        h.deliver(&read(ADDR_CTL_FLAG, 20));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.reply_data().len(), 40);
        assert_fpga_unchanged(&before, &h);
    }

    #[test]
    fn the_last_bus_address_is_reachable_but_not_passed() {
        let mut h = Harness::new();
        h.deliver(&read(0xFFFF, 1));
        assert_eq!(h.status(), Error::None);
        h.deliver(&write(0xFFFF, &[0]));
        assert_eq!(h.status(), Error::None);

        let before = fpga_snapshot(&h);
        h.deliver(&read(0xFFFF, 2));
        assert_eq!(h.status(), Error::InvalidPayload);
        h.deliver(&write(0xFFFF, &[0, 0xFFFF]));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_fpga_unchanged(&before, &h);
    }

    #[test]
    fn malformed_requests_are_rejected_without_touching_the_fpga() {
        let mut h = Harness::new();
        let before = fpga_snapshot(&h);

        let mut half_word = write(ADDR_CTL_FLAG, &[CtlFlags::FAILSAFE.bits()]);
        half_word.set_len(size_of::<WriteFpgaMemoryPayload>() + 1);
        let mut short_read = read(ADDR_CTL_FLAG, 1);
        short_read.set_len(size_of::<ReadFpgaMemoryPayload>() - 1);
        let frames = [
            write(ADDR_CTL_FLAG, &[]),
            half_word,
            read(ADDR_CTL_FLAG, 0),
            read(ADDR_CTL_FLAG, 21),
            short_read,
            Frame::from_parts(
                Cmd::ReadFpgaMemory,
                &ReadFpgaMemoryPayload {
                    addr: U16::ZERO,
                    count: U16::new(1),
                },
                &[0],
            ),
        ];
        for (i, frame) in frames.iter().enumerate() {
            h.deliver(frame);
            assert_eq!(h.status(), Error::InvalidPayload, "frame {i}");
            assert_eq!(h.reply_data(), [0u8; 0]);
        }
        assert_fpga_unchanged(&before, &h);
    }
}
