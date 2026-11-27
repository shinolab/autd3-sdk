use autd3_cpu_wire::payload::TDC_SAMPLES;

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TdcCapture {
    pub capturing: bool,
    pub locked: bool,
    pub samples: [u16; TDC_SAMPLES],
}
