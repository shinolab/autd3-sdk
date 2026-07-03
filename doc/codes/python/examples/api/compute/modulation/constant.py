from autd3_modulation import constant, modulation_buffer

dst = modulation_buffer()
intensity = 0xFF
# ANCHOR: api
constant(intensity, dst)
# ANCHOR_END: api
