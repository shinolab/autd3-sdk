import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3.value import Intensity, Phase
from autd3_pattern import FocusOption, focus, wavelength

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

target = geometry.center() + np.array([0.0, 0.0, 150.0])
wl = wavelength(340 * m / s)
option = (
    # ANCHOR: option
    FocusOption(
        intensity=Intensity.MAX,
        phase_offset=Phase.ZERO,
    )
    # ANCHOR_END: option
)
out = geometry.pattern_buffer()

# ANCHOR: api
focus(geometry, target, wl, option, out)
# ANCHOR_END: api
