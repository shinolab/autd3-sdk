use crate::app::Cpu;
use crate::fpga;
use crate::port::Port;

impl Cpu {
    pub(crate) fn clear<P: Port>(&self, port: &mut P) -> u8 {
        let err = fpga::init(port, self.mode());
        self.silencer.init();
        self.reset_telemetry();
        err
    }
}
