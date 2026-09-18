import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3_pattern import twin_trap, wavelength

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

dst = geometry.pattern_buffer()

twin_trap(
    geometry,
    geometry.center() + np.array([0.0, 0.0, 150.0]),
    np.array([1.0, 0.0, 0.0]),
    wavelength(340 * m / s),
    dst,
)
