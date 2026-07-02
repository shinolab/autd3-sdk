import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3.value import Intensity, Phase
from autd3_pattern import PlaneOption, plane, wavelength

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

direction = np.array([0.0, 0.0, 1.0])
wl = wavelength(340 * m / s)
option = (
    # ANCHOR: option
    PlaneOption(
        intensity=Intensity.MAX,
        phase_offset=Phase.ZERO,
    )
    # ANCHOR_END: option
)
out = geometry.pattern_buffer()

# ANCHOR: api
plane(geometry, direction, wl, option, out)
# ANCHOR_END: api
