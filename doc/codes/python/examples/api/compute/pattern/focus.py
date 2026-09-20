import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3_pattern import focus, wavelength as calc_wavelength

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

target = geometry.center() + np.array([0.0, 0.0, 150.0])
wavelength = calc_wavelength(340 * m / s)
phases = geometry.phase_buffer()

# ANCHOR: api
focus(geometry, target, wavelength, phases)
# ANCHOR_END: api
