mod fifo;
mod health;

use core::cell::Cell;
use core::sync::atomic::Ordering;

use crate::cmd;
use crate::fpga;
use crate::port::Port;
use crate::proto::{
    Cmd, Drained, EXEC_AT_UNIT_NS, Error, FrameHeader, PAYLOAD_BYTES, Reply, ReplyData,
};
use fifo::{FIFO_DEPTH, Fifo, Slot};
use health::Health;

pub struct Cpu {
    fifo: Fifo<Slot>,
    health: Health,
    pub(crate) config: Cell<cmd::cpu_config::CpuConfig>,
    pub(crate) update: cmd::update::UpdateSession,
    pub(crate) fpga_update: cmd::fpga_update::FpgaUpdateSession,
}

const _: () = assert!(FIFO_DEPTH as usize > autd3_cpu_wire::udp::DEVICE_QUEUE_FRAMES);

pub(crate) const EXEC_SPIN_NS: i64 = 50_000;

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    #[must_use]
    #[const_fn::const_fn(cfg(not(loom)))]
    pub const fn new() -> Self {
        Self {
            fifo: Fifo::new([const { Slot::new() }; FIFO_DEPTH as usize]),
            health: Health::new(),
            config: Cell::new(cmd::cpu_config::CpuConfig::DEFAULT),
            update: cmd::update::UpdateSession::new(),
            fpga_update: cmd::fpga_update::FpgaUpdateSession::new(),
        }
    }
}

impl Cpu {
    pub fn init<P: Port>(&self, port: &mut P) {
        let config = self.config();
        if fpga::init(port).is_ok() {
            self.lower_failsafe();
        } else {
            self.count_boot_failure();
        }
        fpga::set_watchdog_timeout(port, config.cpu_watchdog_timeout);
    }

    pub fn mark_boot_attempt<P: Port>(&self, port: &mut P) {
        if self.record_boot_attempt(port).is_err() {
            self.count_boot_failure();
        }
    }

    pub fn tick_1ms<P: Port>(&self, port: &mut P) {
        fpga::watchdog_kick(port);
        self.update_tick(port);
        self.fpga_update_tick(port);
        self.check_failsafes(port);
    }

    #[must_use]
    pub fn reply(&self) -> Reply {
        self.fifo.last_popped().reply.get()
    }

    pub fn recv_frame(&self, frame: &[u8], msg_id: u16) {
        let Some((header, payload)) = FrameHeader::parse(frame) else {
            return;
        };
        if !self.fifo.push(|slot| slot.store(*header, payload, msg_id)) {
            self.health.fifo_drop.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn process_one<P: Port>(&self, port: &mut P) -> Drained {
        let Some(front) = self.fifo.front() else {
            return Drained::Empty;
        };
        let header = front.header.get();
        if !self.is_due(port, header.exec_at.get()) {
            return Drained::Empty;
        }
        let msg_id = front.msg_id.get();
        let Some(reply) = self.handle_slot(port, &front, header.cmd) else {
            self.health.fifo_drop.fetch_add(1, Ordering::Relaxed);
            front.reply.set(self.reply());
            front.pop();
            return Drained::Empty;
        };
        front.reply.set(reply);
        front.pop();
        Drained::Completed { msg_id }
    }

    fn handle_slot<P: Port>(&self, port: &mut P, slot: &Slot, cmd: u8) -> Option<Reply> {
        let payload = slot.payload.try_borrow().ok()?;
        let len = usize::from(slot.len.get()).min(PAYLOAD_BYTES);
        Some(self.handle_frame(port, cmd, &payload.0[..len]))
    }

    fn is_due<P: Port>(&self, port: &mut P, exec_at: u32) -> bool {
        fn ahead_of_exec_at_ns(now: u64, exec_at: u32) -> i64 {
            let ahead_us = exec_at
                .wrapping_sub((now / EXEC_AT_UNIT_NS) as u32)
                .cast_signed();
            i64::from(ahead_us) * EXEC_AT_UNIT_NS.cast_signed()
                - (now % EXEC_AT_UNIT_NS).cast_signed()
        }

        if exec_at == 0 {
            return true;
        }
        let Some(now) = port.sys_time() else {
            self.health
                .exec_late
                .set(self.health.exec_late.get().wrapping_add(1));
            return true;
        };
        if ahead_of_exec_at_ns(now, exec_at) > EXEC_SPIN_NS {
            return false;
        }
        let late = port.without_irq(|port| {
            loop {
                let Some(now) = port.sys_time() else {
                    break true;
                };
                let ahead = ahead_of_exec_at_ns(now, exec_at);
                if ahead <= 0 {
                    break ahead <= -EXEC_AT_UNIT_NS.cast_signed();
                }
            }
        });
        if late {
            self.health
                .exec_late
                .set(self.health.exec_late.get().wrapping_add(1));
        }
        true
    }

    fn handle_frame<P: Port>(&self, port: &mut P, cmd: u8, payload: &[u8]) -> Reply {
        let outcome = match Cmd::from_u8(cmd) {
            Some(cmd) => self.dispatch(port, cmd, payload),
            None => Err(Error::UnknownCmd),
        };
        let result = match outcome {
            Ok(data) => Reply::new(Error::None, data),
            Err(err) => {
                self.health
                    .dispatch_error
                    .set(self.health.dispatch_error.get().wrapping_add(1));
                Reply::new(err, ReplyData::EMPTY)
            }
        };
        self.health
            .processed
            .set(self.health.processed.get().wrapping_add(1));
        result
    }

    fn dispatch<P: Port>(
        &self,
        port: &mut P,
        cmd: Cmd,
        payload: &[u8],
    ) -> Result<ReplyData, Error> {
        if self.fpga_update.is_locked() && !cmd::fpga_update::allowed_while_locked(cmd) {
            return Err(Error::FpgaUpdateInProgress);
        }
        let config = self.config();
        let result = match cmd {
            Cmd::Nop => Ok(()),
            Cmd::SetCpuConfig => self.set_cpu_config(port, payload),
            Cmd::ReadFpgaState => return Ok(cmd::fpga_state::read(port)),
            Cmd::ReadTelemetry => return Ok(self.read_telemetry(port)),
            Cmd::ReadFirmwareInfo => return Ok(self.read_firmware_info(port)),
            Cmd::ReadFpgaMemory => return cmd::fpga_memory::read(port, payload),
            Cmd::WriteFpgaMemory => cmd::fpga_memory::write(port, payload),
            Cmd::ReadRunningImage => {
                return self
                    .running_image(port)
                    .map(|image| ReplyData::from_slice(&[image.as_u8()]));
            }
            Cmd::WriteFociBuffer => cmd::write_foci::handle(port, payload),
            Cmd::WritePatternRaw => cmd::write_pattern_raw::handle(port, payload),
            Cmd::WritePatternFused => cmd::write_pattern_fused::handle(port, payload),
            Cmd::WritePatternPhase => cmd::write_pattern_phase::handle(port, payload),
            Cmd::WriteModulationBuffer => cmd::write_mod::handle(port, payload),
            Cmd::ConfigModulation => cmd::config_mod::handle(port, payload),
            Cmd::ConfigPattern => cmd::config_pattern::handle(port, payload),
            Cmd::ActivateModulationBank => cmd::activate_mod_bank::handle(port, payload),
            Cmd::ActivatePatternBank => cmd::activate_pattern_bank::handle(port, payload),
            Cmd::SetSilencer => cmd::silencer::handle(port, payload),
            Cmd::SetPhaseCorrection => cmd::phase_corr::handle(port, payload),
            Cmd::SetOutputMask => cmd::output_mask::handle(port, payload),
            Cmd::SetPulseWidthTable => cmd::pwe::handle(port, payload),
            Cmd::EmulateGpioIn => cmd::gpio_in::handle(port, payload),
            Cmd::SetGpioOut => cmd::gpio_out::handle(port, payload),
            Cmd::ForceFan => cmd::force_fan::handle(port, payload),
            Cmd::SetFpgaConfig => cmd::fpga_config::handle(port, payload),
            Cmd::ReleaseFailsafe => {
                cmd::failsafe::release(port, self, &config).inspect(|()| self.lower_failsafe())
            }
            Cmd::UpdateBegin => self.update_begin(port, payload),
            Cmd::UpdateChunk => self.update_chunk(port, payload),
            Cmd::UpdateCommit => self.update_commit(port),
            Cmd::Reboot => {
                self.reboot();
                Ok(())
            }
            Cmd::UpdateConfirm => self.update_confirm(port),
            Cmd::FpgaUpdateBegin => self.fpga_update_begin(port, payload),
            Cmd::FpgaUpdateChunk => self.fpga_update_chunk(port, payload),
            Cmd::FpgaUpdateCommit => self.fpga_update_commit(port),
            Cmd::FpgaUpdateActivate => self.fpga_update_activate(),
            Cmd::Synchronize => cmd::sync::handle(port, &config),
            Cmd::Clear => self.clear(port),
            _ => unreachable!(),
        };
        result.map(|()| ReplyData::EMPTY)
    }
}

#[cfg(all(test, not(loom)))]
mod tests {

    use super::EXEC_SPIN_NS;
    use super::fifo::FIFO_DEPTH;
    use crate::cmd::config_mod::ConfigModPayload;
    use crate::cmd::force_fan::ForceFanPayload;
    use crate::fpga::{PHASE_CORR_WORDS, PWE_TABLE_SIZE, REP_INFINITE};
    use crate::fpga_params::{
        ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0, ADDR_MOD_REP0, ADDR_PATTERN_CYCLE0,
        ADDR_PATTERN_MODE0, ADDR_PATTERN_REP0, ADDR_SILENCER_COMPLETION_STEPS_INTENSITY,
        ADDR_SILENCER_COMPLETION_STEPS_PHASE, ADDR_SILENCER_FLAG, ADDR_SILENCER_SET_RESULT,
        ADDR_SILENCER_UPDATE_RATE_INTENSITY, ADDR_SILENCER_UPDATE_RATE_PHASE, CtlFlags,
        EmissionType, NUM_BANKS, NUM_TRANSDUCERS,
    };
    use crate::proto::{Cmd, Drained, EXEC_AT_UNIT_NS, Error, FRAME_BYTES_MAX, OUTPUT_MASK_WORDS};
    use crate::test_utils::builders::{config_mod, force_fan, write_foci_buffer};
    use crate::test_utils::mock::{Frame, Harness};

    #[test]
    fn unknown_cmd_reports_unknown_cmd_and_is_processed() {
        let mut h = Harness::new();
        h.deliver(&Frame::raw(0x7F));
        assert_eq!(h.status(), Error::UnknownCmd);
        assert_eq!(h.processed(), 1);
        assert_eq!(h.telemetry().dispatch_error, 1);
    }

    #[test]
    fn every_cmd_has_a_dispatch_arm() {
        for &cmd in Cmd::ALL {
            let mut h = Harness::new();
            h.deliver(&Frame::new(cmd));
            assert_ne!(h.status(), Error::UnknownCmd, "{cmd:?}");
        }
    }

    #[test]
    fn nop_completes_without_changing_state() {
        let mut h = Harness::new();

        h.deliver(&Frame::new(Cmd::Nop));
        assert_eq!(h.processed(), 1);
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.telemetry().dispatch_error, 0);
    }

    #[test]
    fn unknown_non_streaming_cmd_reports_unknown_cmd() {
        let mut h = Harness::new();
        h.deliver(&Frame::raw(0xEE));
        assert_eq!(h.status(), Error::UnknownCmd);
    }

    #[test]
    fn consecutive_frames_each_process_immediately() {
        let mut h = Harness::new();

        h.deliver(&Frame::new(Cmd::Nop));
        assert_eq!(h.processed(), 1);
        h.deliver(&Frame::new(Cmd::Nop));
        assert_eq!(h.processed(), 2);
        h.deliver(&Frame::new(Cmd::Nop));
        assert_eq!(h.processed(), 3);
    }

    #[test]
    fn fixed_length_payloads_must_match_exactly() {
        let mut h = Harness::new();
        let mut long = force_fan(1);
        long.set_payload_byte(size_of::<ForceFanPayload>(), 0);
        h.deliver(&long);
        assert_eq!(h.status(), Error::InvalidPayload);

        let mut short = config_mod(0, 10, 4);
        short.set_len(size_of::<ConfigModPayload>() - 1);
        h.deliver(&short);
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&force_fan(1));
        assert_eq!(h.status(), Error::None);
    }

    #[test]
    fn an_oversized_frame_is_dropped() {
        let mut h = Harness::new();
        let mut frame = std::vec![0u8; FRAME_BYTES_MAX + 1];
        frame[1] = Cmd::Nop as u8;
        h.cpu.recv_frame(&frame, 0);
        assert!(!h.process_one());
        assert_eq!(h.processed(), 0);
    }

    #[test]
    fn fifo_mode_defers_processing_until_drained() {
        let mut h = Harness::new();

        h.deliver_no_drain(&Frame::new(Cmd::Nop));
        assert_eq!(h.processed(), 0);

        while h.process_one() {}
        assert_eq!(h.processed(), 1);
    }

    #[test]
    fn fifo_mode_drains_in_order() {
        let h = Harness::new();
        let mut port = crate::test_utils::mock::MockPort::new();
        for msg_id in 1..=3 {
            h.cpu.recv_frame(&Frame::new(Cmd::Nop).bytes(), msg_id);
        }
        for msg_id in 1..=3 {
            assert_eq!(h.cpu.process_one(&mut port), Drained::Completed { msg_id });
        }
        assert_eq!(h.cpu.process_one(&mut port), Drained::Empty);
    }

    #[test]
    fn a_duplicated_frame_is_processed_twice() {
        let mut h = Harness::new();
        let frame = Frame::new(Cmd::Nop);
        h.deliver_no_drain(&frame);
        h.deliver_no_drain(&frame);
        while h.process_one() {}
        assert_eq!(h.processed(), 2);
    }

    #[test]
    fn the_reserved_header_byte_is_ignored() {
        let mut h = Harness::new();
        let mut bytes = Frame::new(Cmd::ReadFpgaState).bytes();
        bytes[0] = 0xFF;
        h.cpu.recv_frame(&bytes, 0);
        while h.process_one() {}
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.processed(), 1);
        assert_eq!(h.reply_data().len(), 1);
    }

    #[test]
    fn the_reply_follows_every_completed_frame_across_the_ring_wrap() {
        let mut h = Harness::new();
        for i in 0..(3 * u32::from(FIFO_DEPTH)) {
            h.deliver(&Frame::new(Cmd::ReadFpgaState));
            assert_eq!(h.processed(), 2 * i + 1);
            assert_eq!(h.reply_data().len(), 1);
            h.deliver(&Frame::new(Cmd::Nop));
            assert_eq!(h.reply_data(), []);
        }
    }

    #[test]
    fn the_reply_is_empty_until_the_first_frame_completes() {
        let mut h = Harness::new();
        assert_eq!(h.cpu.reply(), crate::proto::Reply::EMPTY);
        for _ in 0..=FIFO_DEPTH {
            h.deliver(&Frame::new(Cmd::ReadFpgaState));
            assert_eq!(h.reply_data().len(), 1);
        }
        h.reboot();
        assert_eq!(h.cpu.reply(), crate::proto::Reply::EMPTY);
    }

    #[test]
    fn the_isr_sees_the_previous_result_while_a_frame_is_being_processed() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(Cmd::ReadFirmwareInfo));
        let previous = h.cpu.reply();

        h.deliver_no_drain(&write_foci_buffer(0, 0, &[0x5A5A]));
        h.arm_isr_frame(Cmd::Nop);
        assert!(h.process_one());

        assert_eq!(h.port.isr_seen_reply, Some(previous));
        assert_eq!(h.processed(), 2);
        assert_eq!(h.reply_data(), []);
    }

    #[test]
    fn fifo_overflow_drops_beyond_capacity_and_accepts_after_drain() {
        let mut h = Harness::new();

        let capacity = u32::from(FIFO_DEPTH - 1);
        for _ in 0..capacity {
            h.deliver_no_drain(&Frame::new(Cmd::Nop));
        }
        h.deliver_no_drain(&Frame::new(Cmd::Nop));

        while h.process_one() {}
        assert_eq!(h.processed(), capacity);
        assert_eq!(h.telemetry().fifo_drop, 1);

        h.deliver(&Frame::new(Cmd::Nop));
        assert_eq!(h.processed(), capacity + 1);
    }

    #[test]
    fn a_frame_into_a_slot_still_borrowed_is_dropped_and_counted() {
        let h = Harness::new();
        let borrowed = h.cpu.fifo.slots()[0].payload.borrow();
        h.cpu.recv_frame(&Frame::new(Cmd::Nop).bytes(), 1);
        drop(borrowed);
        assert_eq!(h.telemetry().fifo_drop, 1);

        let mut port = crate::test_utils::mock::MockPort::new();
        assert_eq!(h.cpu.process_one(&mut port), Drained::Empty);
        h.cpu.recv_frame(&Frame::new(Cmd::Nop).bytes(), 2);
        assert_eq!(
            h.cpu.process_one(&mut port),
            Drained::Completed { msg_id: 2 }
        );
    }

    #[test]
    fn a_queued_frame_whose_slot_is_still_borrowed_is_dropped_and_counted() {
        let mut h = Harness::new();
        h.deliver(&Frame::new(Cmd::ReadFpgaState));
        let previous = h.cpu.reply();
        h.cpu.recv_frame(&Frame::new(Cmd::Nop).bytes(), 1);
        h.cpu.recv_frame(&Frame::new(Cmd::Nop).bytes(), 2);

        let borrowed = h.cpu.fifo.slots()[1].payload.borrow_mut();
        assert_eq!(h.cpu.process_one(&mut h.port), Drained::Empty);
        drop(borrowed);
        assert_eq!(h.telemetry().fifo_drop, 1);
        assert_eq!(h.processed(), 1);
        assert_eq!(h.cpu.reply(), previous);

        assert_eq!(
            h.cpu.process_one(&mut h.port),
            Drained::Completed { msg_id: 2 }
        );
        assert_eq!(h.processed(), 2);
    }

    #[test]
    fn a_fifo_frame_completes_with_its_msg_id() {
        let h = Harness::new();
        let frame = Frame::new(Cmd::Nop).bytes();
        h.cpu.recv_frame(&frame, 0x1234);
        let mut port = crate::test_utils::mock::MockPort::new();
        assert_eq!(
            h.cpu.process_one(&mut port),
            Drained::Completed { msg_id: 0x1234 }
        );
        assert_eq!(h.cpu.process_one(&mut port), Drained::Empty);
    }

    #[test]
    fn a_full_fifo_drops_the_frame_without_a_reply() {
        let mut h = Harness::new();
        for _ in 0..FIFO_DEPTH - 1 {
            h.cpu.recv_frame(&[0, Cmd::Nop as u8, 0, 0, 0, 0], 0);
        }
        assert_eq!(h.telemetry().fifo_drop, 0);
        h.cpu.recv_frame(&[0, Cmd::Nop as u8, 0, 0, 0, 0], 0);
        assert_eq!(h.telemetry().fifo_drop, 1);
        while h.process_one() {}
        assert_eq!(h.processed(), u32::from(FIFO_DEPTH - 1));
    }

    fn scheduled(exec_at: u32) -> Frame {
        let mut frame = Frame::new(Cmd::Nop);
        frame.set_exec_at(exec_at);
        frame
    }

    const US: u64 = EXEC_AT_UNIT_NS;
    const LATER: u32 = 1_000;
    const SPIN: u64 = EXEC_SPIN_NS.unsigned_abs();

    #[test]
    fn a_frame_beyond_the_spin_window_returns_to_the_main_loop() {
        let mut h = Harness::new();
        let exec_at = u64::from(LATER) * US;
        h.port.sys_time = Some(exec_at - SPIN - 1);
        h.deliver(&scheduled(LATER));
        assert_eq!(h.processed(), 0);
        assert!(!h.process_one());
        assert_eq!(h.port.sys_time_reads_masked, 0);

        h.port.sys_time = Some(exec_at);
        assert!(h.process_one());
        assert_eq!(h.processed(), 1);
        assert_eq!(h.telemetry().exec_late, 0);
    }

    #[test]
    fn a_frame_within_the_spin_window_waits_with_irq_masked_until_its_exec_at() {
        let mut h = Harness::new();
        let exec_at = u64::from(LATER) * US;
        h.port.sys_time_reads = [Some(exec_at - SPIN), Some(exec_at - 1), Some(exec_at)].into();
        h.deliver(&scheduled(LATER));
        assert_eq!(h.processed(), 1);
        assert_eq!(h.port.sys_time_reads_masked, 2);
        assert!(!h.port.irq_masked);
        assert_eq!(h.telemetry().exec_late, 0);
    }

    #[test]
    fn a_lost_device_time_during_the_spin_processes_the_frame_as_late() {
        let mut h = Harness::new();
        let exec_at = u64::from(LATER) * US;
        h.port.sys_time_reads = [Some(exec_at - 1), None].into();
        h.deliver(&scheduled(LATER));
        assert_eq!(h.processed(), 1);
        assert_eq!(h.telemetry().exec_late, 1);
    }

    #[test]
    fn a_zero_exec_at_is_processed_without_reading_the_time() {
        let mut h = Harness::new();
        h.port.sys_time = None;
        h.deliver(&scheduled(0));
        assert_eq!(h.processed(), 1);
        assert_eq!(h.telemetry().exec_late, 0);
    }

    #[test]
    fn a_waiting_frame_holds_back_the_frames_behind_it() {
        let mut h = Harness::new();
        h.port.sys_time = Some(0);
        h.deliver(&scheduled(LATER));
        h.cpu.recv_frame(&scheduled(0).bytes(), 0);
        assert!(!h.process_one());
        assert_eq!(h.processed(), 0);

        h.port.sys_time = Some(u64::from(LATER) * US);
        while h.process_one() {}
        assert_eq!(h.processed(), 2);
    }

    #[test]
    fn the_main_loop_keeps_ticking_while_a_frame_waits() {
        let mut h = Harness::new();
        h.port.sys_time = Some(0);
        h.deliver(&scheduled(LATER));
        let kicks = h.port.watchdog_kicks;
        h.tick_1ms(3);
        assert!(!h.process_one());
        assert_eq!(h.port.watchdog_kicks, kicks + 3);
        assert_eq!(h.processed(), 0);
    }

    #[test]
    fn a_frame_processed_a_microsecond_after_its_exec_at_is_late() {
        let mut h = Harness::new();
        h.port.sys_time = Some(11 * US - 1);
        h.deliver(&scheduled(10));
        assert_eq!(h.telemetry().exec_late, 0);

        h.port.sys_time = Some(12 * US);
        h.deliver(&scheduled(11));
        assert_eq!(h.processed(), 2);
        assert_eq!(h.telemetry().exec_late, 1);
    }

    #[test]
    fn a_frame_is_processed_and_late_when_the_device_time_is_unknown() {
        let mut h = Harness::new();
        h.port.sys_time = None;
        h.deliver(&scheduled(10));
        assert_eq!(h.processed(), 1);
        assert_eq!(h.telemetry().exec_late, 1);
    }

    #[test]
    fn exec_at_compares_across_the_u32_wraparound() {
        let mut h = Harness::new();
        let before_wrap = u64::from(u32::MAX) * US;
        h.port.sys_time = Some(before_wrap);
        h.deliver(&scheduled(LATER));
        assert_eq!(h.processed(), 0);

        h.port.sys_time = Some(before_wrap + u64::from(LATER + 1) * US);
        assert!(h.process_one());
        assert_eq!(h.processed(), 1);
        assert_eq!(h.telemetry().exec_late, 0);

        h.deliver(&scheduled(u32::MAX));
        assert_eq!(h.processed(), 2);
        assert_eq!(h.telemetry().exec_late, 1);
    }

    #[test]
    fn clear_resets_exec_late() {
        let mut h = Harness::new();
        h.port.sys_time = None;
        h.deliver(&scheduled(10));
        assert_eq!(h.telemetry().exec_late, 1);
        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.telemetry().exec_late, 0);
    }

    #[test]
    fn boot_brings_fpga_to_legacy_clear_baseline() {
        let h = Harness::new();

        assert_eq!(h.ctl(ADDR_SILENCER_FLAG), 0);
        assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_INTENSITY), 256);
        assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_PHASE), 256);
        assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 10);
        assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_PHASE), 40);

        for bank in 0..u8::try_from(NUM_BANKS).unwrap() {
            assert_eq!(h.ctl(ADDR_MOD_CYCLE0 + u16::from(bank)), 1);
            assert_eq!(h.ctl(ADDR_MOD_FREQ_DIV0 + u16::from(bank)), 0xFFFF);
            assert_eq!(h.ctl(ADDR_MOD_REP0 + u16::from(bank)), REP_INFINITE);
            assert_eq!(h.mod_word(bank, 0), 0xFFFF);
        }

        for bank in 0..u8::try_from(NUM_BANKS).unwrap() {
            assert_eq!(
                h.ctl(ADDR_PATTERN_MODE0 + u16::from(bank)),
                u16::from(EmissionType::Raw.as_u8())
            );
            assert_eq!(h.ctl(ADDR_PATTERN_CYCLE0 + u16::from(bank)), 0);
            assert_eq!(h.ctl(ADDR_PATTERN_REP0 + u16::from(bank)), REP_INFINITE);
            assert_eq!(h.emission_word(bank, 0), 0);
            assert_eq!(h.emission_word(bank, NUM_TRANSDUCERS - 1), 0);
        }

        assert_eq!(h.port.phase_corr[0], 0);
        assert_eq!(h.port.phase_corr[PHASE_CORR_WORDS - 1], 0);
        assert_eq!(h.port.output_mask[0], 0xFFFF);
        assert_eq!(h.port.output_mask[OUTPUT_MASK_WORDS - 1], 0xFFFF);

        assert_eq!(h.port.pwe[0], 0x00);
        assert_eq!(h.port.pwe[1], 0x01);
        assert_eq!(h.port.pwe[128], 0x56);
        assert_eq!(h.port.pwe[PWE_TABLE_SIZE - 1], 0x100);

        assert_eq!(h.latch_count(CtlFlags::MOD_SET), 1);
        assert_eq!(h.latch_count(CtlFlags::PATTERN_SET), 1);
        assert_eq!(h.latch_count(CtlFlags::SILENCER_SET), 1);
        assert_eq!(h.latch_count(CtlFlags::DEBUG_SET), 1);
        assert_eq!(h.ctl(ADDR_SILENCER_SET_RESULT), 0);
        assert_eq!(h.latch_count(CtlFlags::SYNC_SET), 0);
    }
}
