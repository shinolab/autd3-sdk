import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3_pattern import vortex, wavelength as calc_wavelength

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

target = geometry.center() + np.array([0.0, 0.0, 150.0])
axis = np.array([0.0, 0.0, 1.0])
order = 1
wavelength = calc_wavelength(340 * m / s)
dst = geometry.pattern_buffer()

# ANCHOR: api
vortex(geometry, target, axis, order, wavelength, dst)
# ANCHOR_END: api
