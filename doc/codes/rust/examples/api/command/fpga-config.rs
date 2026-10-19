use autd3_rs::commands::{FpgaConfig, SetFpgaConfig};

fn main() {
    // ANCHOR: api
    let config = FpgaConfig {
        sync_phase_lock: true,
    };
    SetFpgaConfig::new(config);
    // ANCHOR_END: api
}
