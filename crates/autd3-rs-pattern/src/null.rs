use autd3_rs_core::value::Emission;

#[must_use]
#[inline]
pub fn null_transducer() -> Emission {
    Emission::default()
}

pub fn null_device(dst: &mut [Emission]) {
    for slot in dst.iter_mut() {
        *slot = null_transducer();
    }
}

pub fn null(dst: &mut [Vec<Emission>]) {
    for slot in &mut *dst {
        null_device(slot);
    }
}
