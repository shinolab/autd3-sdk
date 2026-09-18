from autd3.geometry import Autd3, Geometry
from autd3.value import Intensity, Phase
from autd3_pattern import add_phase, set_intensity, set_phase, set_phase_and_intensity

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

intensity = Intensity(0x80)
phase = Phase.PI
dst = geometry.pattern_buffer()

# ANCHOR: set_intensity
set_intensity(intensity, dst)
# ANCHOR_END: set_intensity

# ANCHOR: set_phase
set_phase(phase, dst)
# ANCHOR_END: set_phase

# ANCHOR: set_phase_and_intensity
set_phase_and_intensity(phase, intensity, dst)
# ANCHOR_END: set_phase_and_intensity

# ANCHOR: add_phase
add_phase(phase, dst)
# ANCHOR_END: add_phase
