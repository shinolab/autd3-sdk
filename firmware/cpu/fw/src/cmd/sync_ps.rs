use autd3_cpu_wire::payload::{ReadSyncPsStatsPayload, ReadTdcPayload, StartTdcCapturePayload};
use zerocopy::IntoBytes;

use crate::fpga;
use crate::port::Port;
use crate::proto::{Error, ReplyData};

pub(crate) fn read_stats<P: Port>(port: &mut P, payload: &[u8]) -> Result<ReplyData, Error> {
    let p = ReadSyncPsStatsPayload::parse(payload)?;
    let stats = fpga::sync_ps(port).snapshot_stats(p.clear, fpga::FPGA_WAIT_UPDATE_MAX_POLLS)?;
    Ok(ReplyData::from_slice(stats.as_bytes()))
}

pub(crate) fn start_tdc_capture<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let p = StartTdcCapturePayload::parse(payload)?;
    fpga::sync_ps(port).start_tdc_capture(p.source, fpga::FPGA_WAIT_UPDATE_MAX_POLLS)
}

pub(crate) fn read_tdc<P: Port>(port: &mut P, payload: &[u8]) -> Result<ReplyData, Error> {
    let p = ReadTdcPayload::parse(payload)?;
    Ok(ReplyData::from_slice(
        fpga::sync_ps(port).read_tdc(p.chunk).as_bytes(),
    ))
}

#[cfg(all(test, not(loom)))]
mod tests {
    use autd3_cpu_wire::payload::{SyncPsStats, TdcChunk};
    use zerocopy::{FromBytes, TryFromBytes};

    use crate::fpga_params::{
        ADDR_SYNC_PS_DIFF_MAX, ADDR_SYNC_PS_DIFF_MIN, ADDR_SYNC_PS_DIFF_OUTLIERS,
        ADDR_SYNC_PS_FINE_EXITS, ADDR_SYNC_PS_RATE_MAX, ADDR_SYNC_PS_RATE_MIN,
        ADDR_SYNC_PS_STATS_CTL, ADDR_SYNC_PS_TDC_BUF,
    };
    use crate::proto::{Cmd, Error};
    use crate::test_utils::mock::{Frame, Harness};

    fn reg(addr: u16) -> usize {
        usize::from(addr)
    }

    #[test]
    fn stats_are_snapshotted_then_read_back() {
        for (clear, command) in [(false, 0b01), (true, 0b11)] {
            let mut h = Harness::new();
            h.port.sync_ps_regs[reg(ADDR_SYNC_PS_DIFF_MIN)] = (-3i16).cast_unsigned();
            h.port.sync_ps_regs[reg(ADDR_SYNC_PS_DIFF_MAX)] = 4;
            h.port.sync_ps_regs[reg(ADDR_SYNC_PS_DIFF_OUTLIERS)] = 5;
            h.port.sync_ps_regs[reg(ADDR_SYNC_PS_RATE_MIN)] = (-6i16).cast_unsigned();
            h.port.sync_ps_regs[reg(ADDR_SYNC_PS_RATE_MAX)] = 7;
            h.port.sync_ps_regs[reg(ADDR_SYNC_PS_FINE_EXITS)] = 8;
            h.deliver(&Frame::from_payload(
                Cmd::ReadSyncPsStats,
                &[u8::from(clear)],
            ));
            assert_eq!(h.status(), Error::None);
            assert_eq!(h.port.sync_ps_stats_ctl_writes, [command]);
            let stats = SyncPsStats::read_from_bytes(&h.reply_data()).unwrap();
            assert_eq!(
                [
                    stats.diff_min.get(),
                    stats.diff_max.get(),
                    stats.rate_min.get(),
                    stats.rate_max.get()
                ],
                [-3, 4, -6, 7]
            );
            assert_eq!([stats.diff_outliers.get(), stats.fine_exits.get()], [5, 8]);
        }
    }

    #[test]
    fn a_stats_request_that_never_settles_times_out_without_writing() {
        let mut h = Harness::new();
        h.port.sync_ps_regs[reg(ADDR_SYNC_PS_STATS_CTL)] = 1;
        h.deliver(&Frame::from_payload(Cmd::ReadSyncPsStats, &[0u8]));
        assert_eq!(h.status(), Error::FpgaTimeout);
        assert_eq!(h.port.sync_ps_stats_ctl_writes, [] as [u16; 0]);
    }

    #[test]
    fn a_tdc_capture_is_started_on_the_chosen_source() {
        for (source, command) in [(0u8, 0b0100), (1, 0b1100)] {
            let mut h = Harness::new();
            h.deliver(&Frame::from_payload(Cmd::StartTdcCapture, &[source]));
            assert_eq!(h.status(), Error::None);
            assert_eq!(h.port.sync_ps_stats_ctl_writes, [command]);
        }
    }

    #[test]
    fn a_tdc_chunk_carries_the_status_and_its_sixteen_samples() {
        let mut h = Harness::new();
        h.port.sync_ps_regs[reg(ADDR_SYNC_PS_STATS_CTL)] = 0b110;
        for i in 0..128u16 {
            h.port.sync_ps_regs[reg(ADDR_SYNC_PS_TDC_BUF + i)] = 1000 + i;
        }
        h.deliver(&Frame::from_payload(Cmd::ReadTdc, &[7u8]));
        assert_eq!(h.status(), Error::None);
        let chunk = TdcChunk::try_read_from_bytes(&h.reply_data()).unwrap();
        assert!(chunk.capturing && chunk.locked);
        assert_eq!(
            chunk.samples.map(zerocopy::little_endian::U16::get),
            core::array::from_fn(|i| 1112 + i as u16)
        );
        h.deliver(&Frame::from_payload(Cmd::ReadTdc, &[8u8]));
        assert_eq!(h.status(), Error::InvalidPayload);
    }
}
