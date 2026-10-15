import numpy as np
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import deg, m, mm, s
from autd3_pattern import bessel, wavelength

geometry = Geometry([Autd3()])

phases = geometry.phase_buffer()

bessel(
    geometry,
    geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm),
    np.array([0.0, 0.0, 1.0]),
    18.0 * deg,
    wavelength(340 * m / s),
    phases,
)
