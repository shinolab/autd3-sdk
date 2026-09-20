import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3_pattern import (
    HermiteGaussianOption,
    hermite_gaussian_phase,
    hermite_gaussian_intensity,
    wavelength,
)

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

phases = geometry.phase_buffer()
intensities = geometry.intensity_buffer()

target = geometry.center() + np.array([0.0, 0.0, 150.0])
axis = np.array([0.0, 0.0, 1.0])
x_dir = np.array([1.0, 0.0, 0.0])
option = HermiteGaussianOption(m=1, n=1, waist=10.0)
wl = wavelength(340 * m / s)
hermite_gaussian_phase(geometry, target, axis, x_dir, option, wl, phases)
hermite_gaussian_intensity(geometry, target, axis, x_dir, option, wl, intensities)
