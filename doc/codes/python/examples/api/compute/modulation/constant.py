from autd3_modulation import constant, modulation_buffer

dst = modulation_buffer()
amplitude = 0xFF
# ANCHOR: api
constant(amplitude, dst)
# ANCHOR_END: api
