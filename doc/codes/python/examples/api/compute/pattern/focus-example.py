import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3_pattern import focus, wavelength

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

phases = geometry.phase_buffer()

focus(
    geometry,
    geometry.center() + np.array([0.0, 0.0, 150.0]),
    wavelength(340 * m / s),
    phases,
)
