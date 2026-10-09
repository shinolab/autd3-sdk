use autd3_cpu_wire::udp::SYNC_CYCLE_NS;

use crate::cmd::cpu_config::CpuConfig;
use crate::fpga::{self, sys_time_ticks};
use crate::fpga_params::{ADDR_SYNC_TIME_0, CtlFlags, SYNC_CYCLE_TICKS};
use crate::port::Port;
use crate::proto::Error;

const _: () = assert!(sys_time_ticks(SYNC_CYCLE_NS as u64) == SYNC_CYCLE_TICKS as u64);

const SYNC_ATTEMPTS: usize = 3;

pub(crate) fn handle<P: Port>(port: &mut P, config: &CpuConfig) -> Result<(), Error> {
    for _ in 0..SYNC_ATTEMPTS {
        let next_edge = port
            .next_sync_edge(config.sync_guard.as_nanos() as u32)
            .ok_or(Error::SyncNotReady)?
            .get();
        wait_for_the_cycle_before(port, next_edge, config)?;
        fpga::write_u64(port, ADDR_SYNC_TIME_0, sys_time_ticks(next_edge));
        fpga::set_and_wait_update(port, CtlFlags::SYNC_SET, config.fpga_wait_update_max_polls)?;
        let latched_at = port.sys_time().ok_or(Error::SyncNotReady)?;
        if latched_at < next_edge {
            return Ok(());
        }
    }
    Err(Error::SyncMissed)
}

fn wait_for_the_cycle_before<P: Port>(
    port: &mut P,
    edge: u64,
    config: &CpuConfig,
) -> Result<(), Error> {
    for _ in 0..config.fpga_wait_update_max_polls.get() {
        let now = port.sys_time().ok_or(Error::SyncNotReady)?;
        if now.saturating_add(u64::from(SYNC_CYCLE_NS)) > edge {
            return Ok(());
        }
    }
    Err(Error::SyncNotReady)
}

#[cfg(all(test, not(loom)))]
mod tests {
    use core::num::NonZeroU64;

    use crate::fpga_params::{ADDR_SYNC_TIME_0, CtlFlags};
    use crate::proto::{Cmd, Error, FRAME_BYTES_MAX, REPLY_DATA_BYTES_MAX};
    use crate::test_utils::mock::{Frame, Harness};

    const _: () = assert!(FRAME_BYTES_MAX == 1448);
    const _: () = assert!(REPLY_DATA_BYTES_MAX == 40);

    const EDGE: u64 = 1_700_000_000_123_000_000;
    const CYCLE: u64 = 1_000_000;

    fn harness(edges: &[Option<u64>], sys_time_reads: &[Option<u64>]) -> Harness {
        let mut h = Harness::new();
        h.port.next_sync_edges = edges.iter().map(|e| e.and_then(NonZeroU64::new)).collect();
        h.port.sys_time_reads = sys_time_reads.iter().copied().collect();
        h
    }

    fn assert_sync_time(h: &Harness, edge_ns: u64) {
        let ticks = edge_ns / 3125 * 64;
        for i in 0..4 {
            assert_eq!(h.ctl(ADDR_SYNC_TIME_0 + i), (ticks >> (16 * i)) as u16);
        }
    }

    #[test]
    fn synchronize_writes_next_sync_edge_in_sys_time_ticks_and_latches() {
        let mut h = harness(&[Some(EDGE)], &[Some(EDGE - 250_000), Some(EDGE - 240_000)]);

        h.deliver(&Frame::new(0, Cmd::Synchronize));

        assert_eq!(h.ack(), 0);
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_SYNC_TIME_0), 0x7000);
        assert_eq!(h.ctl(ADDR_SYNC_TIME_0 + 1), 0xB0A6);
        assert_eq!(h.ctl(ADDR_SYNC_TIME_0 + 2), 0xB0F7);
        assert_eq!(h.ctl(ADDR_SYNC_TIME_0 + 3), 0x007B);
        assert_eq!(h.latch_count(CtlFlags::SYNC_SET), 1);
        assert!(!h.ctl_flags().contains(CtlFlags::SYNC_SET));
        assert!(h.port.sys_time_reads.is_empty());
    }

    #[test]
    fn synchronize_returns_sync_not_ready_before_the_pulse_runs() {
        let mut h = Harness::new();
        h.port.next_sync_edge = None;

        h.deliver(&Frame::new(0, Cmd::Synchronize));
        assert_eq!(h.status(), Error::SyncNotReady);
        assert_eq!(h.ctl(ADDR_SYNC_TIME_0), 0);
        assert_eq!(h.latch_count(CtlFlags::SYNC_SET), 0);
    }

    #[test]
    fn synchronize_succeeds_when_the_readback_is_just_before_the_edge() {
        let mut h = harness(&[Some(EDGE)], &[Some(EDGE - CYCLE + 1), Some(EDGE - 1)]);

        h.deliver(&Frame::new(0, Cmd::Synchronize));

        assert_eq!(h.status(), Error::None);
        assert_sync_time(&h, EDGE);
        assert_eq!(h.latch_count(CtlFlags::SYNC_SET), 1);
    }

    #[test]
    fn synchronize_waits_until_the_written_edge_is_the_next_one() {
        let mut h = harness(
            &[Some(EDGE)],
            &[
                Some(EDGE - CYCLE - 200_000),
                Some(EDGE - CYCLE - 1),
                Some(EDGE - CYCLE),
                Some(EDGE - CYCLE + 1),
                Some(EDGE - CYCLE + 10_000),
            ],
        );

        h.deliver(&Frame::new(0, Cmd::Synchronize));

        assert_eq!(h.status(), Error::None);
        assert_sync_time(&h, EDGE);
        assert_eq!(h.latch_count(CtlFlags::SYNC_SET), 1);
        assert!(h.port.sys_time_reads.is_empty());
    }

    #[test]
    fn synchronize_returns_sync_not_ready_when_the_clock_never_reaches_the_cycle_before() {
        let mut h = harness(&[Some(EDGE)], &[Some(EDGE - CYCLE)]);

        h.deliver(&Frame::new(0, Cmd::Synchronize));

        assert_eq!(h.status(), Error::SyncNotReady);
        assert_eq!(h.ctl(ADDR_SYNC_TIME_0), 0);
        assert_eq!(h.latch_count(CtlFlags::SYNC_SET), 0);
    }

    #[test]
    fn synchronize_returns_sync_not_ready_when_the_time_is_unavailable_before_the_latch() {
        let mut h = harness(&[Some(EDGE)], &[None]);

        h.deliver(&Frame::new(0, Cmd::Synchronize));

        assert_eq!(h.status(), Error::SyncNotReady);
        assert_eq!(h.latch_count(CtlFlags::SYNC_SET), 0);
    }

    #[test]
    fn synchronize_rewrites_a_fresh_edge_when_the_readback_is_at_the_edge() {
        let mut h = harness(
            &[Some(EDGE), Some(EDGE + CYCLE)],
            &[
                Some(EDGE - 250_000),
                Some(EDGE),
                Some(EDGE + 1),
                Some(EDGE + CYCLE - 1),
            ],
        );

        h.deliver(&Frame::new(0, Cmd::Synchronize));

        assert_eq!(h.status(), Error::None);
        assert_sync_time(&h, EDGE + CYCLE);
        assert_eq!(h.latch_count(CtlFlags::SYNC_SET), 2);
        assert!(!h.ctl_flags().contains(CtlFlags::SYNC_SET));
        assert!(h.port.sys_time_reads.is_empty());
    }

    #[test]
    fn synchronize_succeeds_on_the_last_attempt() {
        let mut h = harness(
            &[Some(EDGE), Some(EDGE + CYCLE), Some(EDGE + 2 * CYCLE)],
            &[
                Some(EDGE - 1),
                Some(EDGE + 1),
                Some(EDGE + 2),
                Some(EDGE + CYCLE),
                Some(EDGE + CYCLE + 1),
                Some(EDGE + 2 * CYCLE - 1),
            ],
        );

        h.deliver(&Frame::new(0, Cmd::Synchronize));

        assert_eq!(h.status(), Error::None);
        assert_sync_time(&h, EDGE + 2 * CYCLE);
        assert_eq!(h.latch_count(CtlFlags::SYNC_SET), 3);
    }

    #[test]
    fn synchronize_returns_sync_missed_after_three_late_latches() {
        let mut h = harness(
            &[
                Some(EDGE),
                Some(EDGE + CYCLE),
                Some(EDGE + 2 * CYCLE),
                Some(EDGE + 3 * CYCLE),
            ],
            &[
                Some(EDGE - 1),
                Some(EDGE),
                Some(EDGE + 1),
                Some(EDGE + CYCLE),
                Some(EDGE + CYCLE + 1),
                Some(EDGE + 2 * CYCLE),
            ],
        );

        h.deliver(&Frame::new(0, Cmd::Synchronize));

        assert_eq!(h.status(), Error::SyncMissed);
        assert_eq!(h.latch_count(CtlFlags::SYNC_SET), 3);
        assert_eq!(h.port.next_sync_edges.len(), 1);
    }

    #[test]
    fn synchronize_returns_sync_not_ready_when_the_readback_time_is_unavailable() {
        let mut h = harness(&[Some(EDGE)], &[Some(EDGE - 1), None]);

        h.deliver(&Frame::new(0, Cmd::Synchronize));

        assert_eq!(h.status(), Error::SyncNotReady);
        assert_eq!(h.latch_count(CtlFlags::SYNC_SET), 1);
    }

    #[test]
    fn synchronize_returns_sync_not_ready_when_the_pulse_stops_before_the_retry() {
        let mut h = harness(&[Some(EDGE), None], &[Some(EDGE - 1), Some(EDGE)]);

        h.deliver(&Frame::new(0, Cmd::Synchronize));

        assert_eq!(h.status(), Error::SyncNotReady);
        assert_eq!(h.latch_count(CtlFlags::SYNC_SET), 1);
    }

    #[test]
    fn set_and_wait_update_times_out_when_latch_stuck() {
        let mut h = Harness::new();
        h.port.latch_stuck = true;

        h.port.next_sync_edge = NonZeroU64::new(0x1122_3344_5566_7788);
        h.port.sys_time = Some(0x1122_3344_5566_7788 - 1);
        h.deliver(&Frame::new(0, Cmd::Synchronize));
        assert_eq!(h.status(), Error::FpgaTimeout);
        assert_eq!(h.latch_count(CtlFlags::SYNC_SET), 1);

        h.port.latch_stuck = false;
    }
}
