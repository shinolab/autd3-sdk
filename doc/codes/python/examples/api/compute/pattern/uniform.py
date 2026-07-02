from autd3.geometry import Autd3, Geometry
from autd3.value import Emission, Intensity, Phase
from autd3_pattern import uniform

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

emission = Emission(
    Phase.ZERO,
    Intensity.MAX,
)
out = geometry.pattern_buffer()

# ANCHOR: api
uniform(emission, out)
# ANCHOR_END: api
