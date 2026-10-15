import numpy as np
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, s
from autd3_pattern import (
    HermiteGaussianOption,
    hermite_gaussian_phase,
    hermite_gaussian_intensity,
    wavelength as calc_wavelength,
)

geometry = Geometry([Autd3()])

target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
axis = np.array([0.0, 0.0, 1.0])
x_dir = np.array([1.0, 0.0, 0.0])
wavelength = calc_wavelength(340 * m / s)
phases = geometry.phase_buffer()
intensities = geometry.intensity_buffer()

# ANCHOR: api
option = HermiteGaussianOption(m=1, n=0, waist=10.0 * mm)
hermite_gaussian_phase(geometry, target, axis, x_dir, option, wavelength, phases)
hermite_gaussian_intensity(geometry, target, axis, x_dir, option, wavelength, intensities)
# ANCHOR_END: api
