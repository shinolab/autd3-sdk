use autd3_rs_core::protocol::Cmd;

pub(crate) use autd3_cpu_wire::fpga_params::NUM_TRANSDUCERS;

pub(crate) fn frame(seq: u8, cmd: Cmd, payload: &[u8]) -> Vec<u8> {
    [&[seq, cmd.as_u8(), 0, 0, 0, 0], payload].concat()
}
