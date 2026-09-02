import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3.value import Intensity, Phase
from autd3_pattern import VortexOption, vortex, wavelength as calc_wavelength

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

target = geometry.center() + np.array([0.0, 0.0, 150.0])
axis = np.array([0.0, 0.0, 1.0])
order = 1
wavelength = calc_wavelength(340 * m / s)
intensity = Intensity.MAX
phase_offset = Phase.ZERO
option = (
    # ANCHOR: option
    VortexOption(
        intensity,
        phase_offset,
    )
    # ANCHOR_END: option
)
dst = geometry.pattern_buffer()

# ANCHOR: api
vortex(geometry, target, axis, order, wavelength, option, dst)
# ANCHOR_END: api
