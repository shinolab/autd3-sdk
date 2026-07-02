from autd3_modulation import constant, modulation_buffer

out = modulation_buffer()
intensity = 0xFF
# ANCHOR: api
constant(intensity, out)
# ANCHOR_END: api
