use core::ffi::c_void;

use autd3_rs_core::protocol::{RX_FRAME_BYTES, RxFrame, TX_FRAME_BYTES};

use crate::ffi;
use crate::fpga::FpgaEmulator;
use crate::port::{ActiveGuard, FW_LOCK};

const WIRE_GAP_START: usize = ffi::WIRE_RX_GAP_START as usize;
const WIRE_RX_FRAME_BYTES: usize = ffi::WIRE_RX_FRAME_BYTES as usize;

pub struct Device {
    handle: *mut c_void,
    fpga: Box<FpgaEmulator>,
}

unsafe impl Send for Device {}

impl Device {
    #[must_use]
    pub fn new(num_transducers: usize) -> Self {
        let mut fpga = Box::new(FpgaEmulator::new(num_transducers));
        let _lock = FW_LOCK.lock().unwrap();

        let handle = unsafe { ffi::emu_device_new() };
        assert!(!handle.is_null(), "failed to allocate emulator device");
        let _active = ActiveGuard::set(&raw mut *fpga);

        unsafe {
            ffi::emu_device_select(handle);
            ffi::init_app();
        }
        Self { handle, fpga }
    }

    pub fn send(&mut self, tx: &[u8; TX_FRAME_BYTES]) -> RxFrame {
        let wire = logical_to_wire(tx);
        let fpga: *mut FpgaEmulator = &raw mut *self.fpga;
        let _lock = FW_LOCK.lock().unwrap();
        let _active = ActiveGuard::set(fpga);

        unsafe {
            ffi::emu_device_select(self.handle);
            ffi::recv_ethercat(wire.as_ptr());
            ffi::app_process_pending();
            let mut rx = [0u8; RX_FRAME_BYTES];
            rx[0] = ffi::emu_tx_ack();
            rx[1] = ffi::emu_tx_data();
            RxFrame::parse(&rx)
        }
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

impl Drop for Device {
    fn drop(&mut self) {
        let _lock = FW_LOCK.lock().unwrap();

        unsafe { ffi::emu_device_free(self.handle) };
    }
}

fn logical_to_wire(tx: &[u8; TX_FRAME_BYTES]) -> [u8; WIRE_RX_FRAME_BYTES] {
    let mut wire = [0u8; WIRE_RX_FRAME_BYTES];
    wire[..WIRE_GAP_START].copy_from_slice(&tx[..WIRE_GAP_START]);
    wire[WIRE_GAP_START + 2..].copy_from_slice(&tx[WIRE_GAP_START..]);
    wire
}
