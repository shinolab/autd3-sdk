use autd3_rs_core::protocol::{RX_FRAME_BYTES, RxFrame, TX_FRAME_BYTES};

use crate::app::Cpu;
use crate::emu_fpga::FpgaEmulator;
use crate::fw;

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

    pub fn send(&mut self, tx: &[u8; TX_FRAME_BYTES]) -> RxFrame {
        let wire = logical_to_wire(tx);
        self.cpu.recv_ethercat(&mut self.fpga, &wire);
        self.cpu.process_pending(&mut self.fpga);

        let tx = self.cpu.tx();
        let mut rx = [0u8; RX_FRAME_BYTES];
        rx[0] = tx.ack;
        rx[1] = tx.data;
        RxFrame::parse(&rx)
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
