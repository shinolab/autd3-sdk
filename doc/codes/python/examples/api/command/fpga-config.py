from autd3.commands import FpgaConfig, SetFpgaConfig

# ANCHOR: api
config = FpgaConfig(
    sync_phase_lock=True,
)
SetFpgaConfig(config)
# ANCHOR_END: api
