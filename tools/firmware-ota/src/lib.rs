mod fpga_image;
mod udp;
mod update;

pub use autd3_firmware_writer::image::{CpuFirmwareImage, ImageError};
pub use fpga_image::FpgaFirmwareImage;
pub use udp::{OpenError, UdpArgs};
pub use update::{
    ACK_TIMEOUT, DEFAULT_HEARTBEAT, ExplicitOperation, FPGA_ACTIVATE_ATTEMPTS, FPGA_RECONFIG_WAIT,
    KeepaliveClient, OtaError, UpdateProgress, activate_fpga, activate_unrebooted,
    activate_unreconfigured_fpga, confirm, ensure_fpga_reconfigured, ensure_no_device_lost,
    ota_error, read_firmware_version, read_running_image, reboot, send, update_cpu, update_fpga,
    version_triple,
};
