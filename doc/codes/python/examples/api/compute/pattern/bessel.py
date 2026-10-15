import numpy as np
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import deg, m, mm, s
from autd3_pattern import bessel, wavelength as calc_wavelength

geometry = Geometry([Autd3()])

apex = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
direction = np.array([0.0, 0.0, 1.0])
theta = 18.0 * deg
wavelength = calc_wavelength(340 * m / s)
phases = geometry.phase_buffer()

# ANCHOR: api
bessel(geometry, apex, direction, theta, wavelength, phases)
# ANCHOR_END: api
