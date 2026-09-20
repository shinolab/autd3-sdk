from autd3.geometry import Autd3, Geometry
from autd3.value import Intensity, Phase
from autd3_pattern import add_phase, set_intensity, set_phase

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

intensity = Intensity(0x80)
phase = Phase.PI
phases = geometry.phase_buffer()
intensities = geometry.intensity_buffer()

# ANCHOR: set_intensity
set_intensity(intensity, intensities)
# ANCHOR_END: set_intensity

# ANCHOR: set_phase
set_phase(phase, phases)
# ANCHOR_END: set_phase

# ANCHOR: add_phase
add_phase(phase, phases)
# ANCHOR_END: add_phase
