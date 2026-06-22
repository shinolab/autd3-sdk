use autd3_rs_core::params::NUM_TRANSDUCERS;
use autd3_rs_core::value::Emission;

pub fn null_transducer(out: &mut Emission) {
    *out = Emission::default();
}

pub fn null_device(out: &mut [Emission; NUM_TRANSDUCERS]) {
    for slot in out.iter_mut() {
        null_transducer(slot);
    }
}

pub fn null(out: &mut [[Emission; NUM_TRANSDUCERS]]) {
    for slot in &mut *out {
        null_device(slot);
    }
}
