import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3.value import Intensity, Phase
from autd3_pattern import add_phase, focus, set_intensity, wavelength

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

phases = geometry.phase_buffer()
intensities = geometry.intensity_buffer()

set_intensity(Intensity(0x80), intensities)
focus(
    geometry,
    geometry.center() + np.array([0.0, 0.0, 150.0]),
    wavelength(340 * m / s),
    phases,
)
add_phase(Phase.PI, phases)

set_intensity(Intensity.MIN, intensities)
