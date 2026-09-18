import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3_pattern import (
    HermiteGaussianOption,
    hermite_gaussian_phase,
    hermite_gaussian_intensity,
    wavelength as calc_wavelength,
)

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

target = geometry.center() + np.array([0.0, 0.0, 150.0])
axis = np.array([0.0, 0.0, 1.0])
x_dir = np.array([1.0, 0.0, 0.0])
wavelength = calc_wavelength(340 * m / s)
dst = geometry.pattern_buffer()

# ANCHOR: api
option = HermiteGaussianOption(m=1, n=0, waist=10.0)
hermite_gaussian_phase(geometry, target, axis, x_dir, option, wavelength, dst)
hermite_gaussian_intensity(geometry, target, axis, x_dir, option, wavelength, dst)
# ANCHOR_END: api
