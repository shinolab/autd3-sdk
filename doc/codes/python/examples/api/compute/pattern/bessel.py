import math

import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, rad, s
from autd3.value import Intensity, Phase
from autd3_pattern import BesselOption, bessel, wavelength

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

apex = geometry.center() + np.array([0.0, 0.0, 150.0])
direction = np.array([0.0, 0.0, 1.0])
theta = math.radians(18.0) * rad
wl = wavelength(340 * m / s)
option = (
    # ANCHOR: option
    BesselOption(
        intensity=Intensity.MAX,
        phase_offset=Phase.ZERO,
    )
    # ANCHOR_END: option
)
out = geometry.pattern_buffer()

# ANCHOR: api
bessel(geometry, apex, direction, theta, wl, option, out)
# ANCHOR_END: api
