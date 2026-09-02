import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3.value import Intensity, Phase
from autd3_pattern import TwinTrapOption, twin_trap, wavelength as calc_wavelength

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

target = geometry.center() + np.array([0.0, 0.0, 150.0])
normal = np.array([1.0, 0.0, 0.0])
wavelength = calc_wavelength(340 * m / s)
intensity = Intensity.MAX
phase_offset = Phase.ZERO
option = (
    # ANCHOR: option
    TwinTrapOption(
        intensity,
        phase_offset,
    )
    # ANCHOR_END: option
)
dst = geometry.pattern_buffer()

# ANCHOR: api
twin_trap(geometry, target, normal, wavelength, option, dst)
# ANCHOR_END: api
