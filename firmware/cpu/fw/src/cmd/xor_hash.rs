use zerocopy::FromBytes;

pub use autd3_cpu_wire::layout::XOR_HASH_MAX_DATA_LEN;
pub use autd3_cpu_wire::payload::XorHashPayload;

use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let Ok((p, rest)) = XorHashPayload::ref_from_prefix(payload) else {
        return Err(Error::InvalidPayload);
    };
    let sleep_ms = p.sleep_ms.get();
    let data_len = usize::from(p.data_len.get());
    if data_len > XOR_HASH_MAX_DATA_LEN {
        return Err(Error::InvalidPayload);
    }
    if sleep_ms != 0 {
        port.sleep_ms(sleep_ms);
    }
    if rest[..data_len].iter().fold(0u8, |h, b| h ^ b) != 0 {
        return Err(Error::InvalidData);
    }
    Ok(())
}
