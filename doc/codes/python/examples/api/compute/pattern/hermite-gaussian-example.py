import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3_pattern import (
    HermiteGaussianOption,
    hermite_gaussian_phase,
    hermite_gaussian_intensity,
)
from autd3_pattern import wavelength as calc_wavelength

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

phases = geometry.phase_buffer()
intensities = geometry.intensity_buffer()

target = geometry.center() + np.array([0.0, 0.0, 150.0])
option = HermiteGaussianOption(m=1, n=1, waist=10.0)
wavelength = calc_wavelength(340 * m / s)
hermite_gaussian_phase(geometry, target, np.array([0.0, 0.0, 1.0]), np.array([1.0, 0.0, 0.0]), option, wavelength, phases)
hermite_gaussian_intensity(geometry, target, np.array([0.0, 0.0, 1.0]), np.array([1.0, 0.0, 0.0]), option, wavelength, intensities)
