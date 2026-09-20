import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3_pattern import plane, wavelength

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

phases = geometry.phase_buffer()

plane(
    geometry,
    np.array([0.0, 0.0, 1.0]),
    wavelength(340 * m / s),
    phases,
)
