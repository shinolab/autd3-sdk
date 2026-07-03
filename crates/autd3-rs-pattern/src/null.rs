use autd3_rs_core::value::Emission;

pub fn null_transducer(dst: &mut Emission) {
    *dst = Emission::default();
}

pub fn null_device(dst: &mut [Emission]) {
    for slot in dst.iter_mut() {
        null_transducer(slot);
    }
}

pub fn null(dst: &mut [Vec<Emission>]) {
    for slot in &mut *dst {
        null_device(slot);
    }
}
