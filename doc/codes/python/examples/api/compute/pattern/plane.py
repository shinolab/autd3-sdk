import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3_pattern import plane, wavelength as calc_wavelength

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

direction = np.array([0.0, 0.0, 1.0])
wavelength = calc_wavelength(340 * m / s)
dst = geometry.pattern_buffer()

# ANCHOR: api
plane(geometry, direction, wavelength, dst)
# ANCHOR_END: api
