use autd3_rs_core::protocol::{RX_FRAME_BYTES, RxFrame, TX_FRAME_BYTES};

use crate::emu_fpga::FpgaEmulator;
use crate::fw;
use autd3_cpu_fw::Cpu;
use autd3_cpu_fw::proto::Mode;

const WIRE_GAP_START: usize = fw::WIRE_RX_GAP_START;
const WIRE_RX_FRAME_BYTES: usize = fw::WIRE_RX_FRAME_BYTES;

pub struct Device {
    cpu: Cpu,
    fpga: FpgaEmulator,
}

impl Device {
    #[must_use]
    pub fn new(num_transducers: usize) -> Self {
        let mut fpga = FpgaEmulator::new(num_transducers);
        let cpu = Cpu::new();
        cpu.init(&mut fpga);
        Self { cpu, fpga }
    }

    pub fn recv(&mut self, tx: &[u8; TX_FRAME_BYTES]) {
        let wire = logical_to_wire(tx);
        self.cpu.recv_ethercat(&mut self.fpga, &wire);
    }

    pub fn process_one(&mut self) -> bool {
        self.cpu.process_one(&mut self.fpga)
    }

    pub fn process_pending(&mut self) {
        self.cpu.process_pending(&mut self.fpga);
    }

    #[must_use]
    pub fn rx(&self) -> RxFrame {
        let tx = self.cpu.tx();
        let mut rx = [0u8; RX_FRAME_BYTES];
        rx[0] = tx.ack;
        rx[1] = tx.data;
        RxFrame::parse(&rx)
    }

    pub fn send(&mut self, tx: &[u8; TX_FRAME_BYTES]) -> RxFrame {
        self.recv(tx);
        self.process_pending();
        self.rx()
    }

    #[must_use]
    pub fn mode(&self) -> Mode {
        self.cpu.mode()
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

fn logical_to_wire(tx: &[u8; TX_FRAME_BYTES]) -> [u8; WIRE_RX_FRAME_BYTES] {
    let mut wire = [0u8; WIRE_RX_FRAME_BYTES];
    wire[..WIRE_GAP_START].copy_from_slice(&tx[..WIRE_GAP_START]);
    wire[WIRE_GAP_START + 2..].copy_from_slice(&tx[WIRE_GAP_START..]);
    wire
}
