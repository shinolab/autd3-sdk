use autd3_rs_core::protocol::Cmd;

pub use autd3_cpu_wire::fpga_params::NUM_TRANSDUCERS;

pub fn frame(seq: u8, cmd: Cmd, payload: &[u8]) -> Vec<u8> {
    [&[seq, cmd.as_u8()], payload].concat()
}
