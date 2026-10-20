use zerocopy::IntoBytes;

use autd3_cpu_wire::payload::FirmwareInfo;

use crate::app::Cpu;
use crate::fpga;
use crate::fpga_params::{
    ADDR_FUNCTION_BITS, ADDR_VERSION_NUM_MAJOR, ADDR_VERSION_NUM_MINOR, ADDR_VERSION_NUM_PATCH,
};
use crate::port::Port;
use crate::proto::ReplyData;
use crate::version::{FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH};

impl Cpu {
    pub(crate) fn read_firmware_info<P: Port>(&self, port: &mut P) -> ReplyData {
        let info = FirmwareInfo {
            cpu_version: [FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH],
            fpga_version: [
                fpga::read_ctl(port, ADDR_VERSION_NUM_MAJOR) as u8,
                fpga::read_ctl(port, ADDR_VERSION_NUM_MINOR) as u8,
                fpga::read_ctl(port, ADDR_VERSION_NUM_PATCH) as u8,
            ],
            fpga_functions: fpga::read_ctl(port, ADDR_FUNCTION_BITS) as u8,
            fpga_boot_image: self.fpga_boot_image(port) as u8,
        };
        ReplyData::from_slice(info.as_bytes())
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use crate::fpga_params::{
        ADDR_FUNCTION_BITS, ADDR_VERSION_NUM_MAJOR, ADDR_VERSION_NUM_MINOR, ADDR_VERSION_NUM_PATCH,
    };
    use crate::proto::{Cmd, Error};
    use crate::test_utils::mock::{Frame, Harness};

    #[test]
    fn read_fpga_functions_returns_function_bits_register() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_FUNCTION_BITS, 0xA5);

        h.deliver(&Frame::new(Cmd::ReadFirmwareInfo));
        assert_eq!(h.firmware_info().fpga_functions, 0xA5);
    }

    #[test]
    fn read_fpga_fw_version_returns_register_bytes() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_VERSION_NUM_MAJOR, 0x0A);
        h.set_ctl(ADDR_VERSION_NUM_MINOR, 0x0B);
        h.set_ctl(ADDR_VERSION_NUM_PATCH, 0x0C);

        h.deliver(&Frame::new(Cmd::ReadFirmwareInfo));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.firmware_info().fpga_version, [0x0A, 0x0B, 0x0C]);
    }
}
