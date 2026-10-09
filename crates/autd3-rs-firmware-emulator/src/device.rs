use crate::emu_fpga::FpgaEmulator;
use autd3_cpu_fw::Cpu;
use autd3_cpu_fw::proto::{Disposition, Drained, Reply, RxFrame};

pub struct Device {
    pub(crate) cpu: Cpu,
    fpga: FpgaEmulator,
    work: Box<RxFrame>,
}

impl Device {
    #[must_use]
    pub fn new(num_transducers: usize) -> Self {
        let mut fpga = FpgaEmulator::new(num_transducers);
        let cpu = boot(&mut fpga);
        Self {
            cpu,
            fpga,
            work: Box::new(RxFrame::ZERO),
        }
    }

    pub fn reset_cpu(&mut self) {
        self.cpu = boot(&mut self.fpga);
    }

    pub fn recv(&mut self, frame: &[u8], msg_id: u16) -> Disposition {
        self.cpu.recv_frame(frame, msg_id)
    }

    pub fn process_one(&mut self) -> Drained {
        self.cpu.process_one(&mut self.fpga, &mut self.work)
    }

    fn process_pending(&mut self) {
        self.cpu.process_pending(&mut self.fpga);
    }

    pub fn tick_1ms(&mut self) {
        let resets = self.fpga.reset_count();
        self.cpu.tick_1ms(&mut self.fpga);
        if self.fpga.reset_count() != resets {
            self.reset_cpu();
        }
    }

    #[must_use]
    pub fn reply(&self) -> Reply {
        self.cpu.reply()
    }

    pub fn send(&mut self, frame: &[u8]) -> Reply {
        let _ = self.recv(frame, 0);
        self.process_pending();
        self.reply()
    }

    #[must_use]
    pub fn fpga(&self) -> &FpgaEmulator {
        &self.fpga
    }

    #[must_use]
    pub fn fpga_mut(&mut self) -> &mut FpgaEmulator {
        &mut self.fpga
    }
}

fn boot(fpga: &mut FpgaEmulator) -> Cpu {
    let cpu = Cpu::new();
    cpu.mark_boot_attempt(fpga);
    cpu.init(fpga);
    cpu
}
