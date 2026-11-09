use autd3_rs::commands::{FpgaConfig, SetFpgaConfig};

fn main() {
    let sync_phase_lock = true;
    let sync_kp = 64;
    let sync_ki = 1024;
    let sync_kf = 4096;
    // ANCHOR: api
    let config = FpgaConfig {
        sync_phase_lock,
        sync_kp,
        sync_ki,
        sync_kf,
    };
    SetFpgaConfig::new(config);
    // ANCHOR_END: api
}
