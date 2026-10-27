from autd3.commands import FpgaConfig, SetFpgaConfig

sync_phase_lock = True
# ANCHOR: api
config = FpgaConfig(
    sync_phase_lock=sync_phase_lock,
)
SetFpgaConfig(config)
# ANCHOR_END: api
