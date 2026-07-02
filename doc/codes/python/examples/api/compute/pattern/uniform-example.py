from autd3.geometry import Autd3, Geometry
from autd3.value import Emission, Intensity, Phase
from autd3_pattern import uniform

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

out = geometry.pattern_buffer()

uniform(
    Emission(
        phase=Phase.ZERO,
        intensity=Intensity.MAX,
    ),
    out,
)
