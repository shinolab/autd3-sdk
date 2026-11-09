from autd3.commands import FpgaConfig, SetFpgaConfig

sync_phase_lock = True
sync_kp = 64
sync_ki = 1024
sync_kf = 4096
# ANCHOR: api
config = FpgaConfig(
    sync_phase_lock=sync_phase_lock,
    sync_kp=sync_kp,
    sync_ki=sync_ki,
    sync_kf=sync_kf,
)
SetFpgaConfig(config)
# ANCHOR_END: api
