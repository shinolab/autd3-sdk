pub use autd3_cpu_wire::payload::PwePayload;

use crate::fpga;
use crate::fpga_params::BramSelect;
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let p = PwePayload::parse(payload)?;
    for (i, value) in p.table.iter().enumerate() {
        fpga::write(port, BramSelect::PweTable, i as u16, value.get());
    }
    Ok(())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use std::vec::Vec;

    use crate::fpga::PWE_TABLE_SIZE;
    use crate::proto::Error;
    use crate::test_utils::builders::pwe;
    use crate::test_utils::mock::Harness;

    #[test]
    fn pwe_writes_table() {
        let mut h = Harness::new();
        let table: Vec<u16> = (0..PWE_TABLE_SIZE).map(|i| i as u16).collect();
        h.deliver(&pwe(0, &table));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.pwe[0], 0);
        assert_eq!(h.port.pwe[1], 1);
        assert_eq!(h.port.pwe[255], 255);
    }
}
