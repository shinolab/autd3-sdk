use crate::app::Cpu;
use crate::fpga;
use crate::port::Port;
use crate::proto::Error;

impl Cpu {
    pub(crate) fn clear<P: Port>(&self, port: &mut P) -> Result<(), Error> {
        let result = fpga::init(port, self.mode());
        self.silencer.init();
        self.reset_telemetry();
        result
    }
}
