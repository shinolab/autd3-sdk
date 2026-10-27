use autd3_rs::commands::{FpgaConfig, SetFpgaConfig};

fn main() {
    let sync_phase_lock = true;
    // ANCHOR: api
    let config = FpgaConfig {
        sync_phase_lock,
    };
    SetFpgaConfig::new(config);
    // ANCHOR_END: api
}
