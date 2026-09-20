import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3_pattern import (
    LaguerreGaussianOption,
    laguerre_gaussian_phase,
    laguerre_gaussian_intensity,
    wavelength,
)

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

phases = geometry.phase_buffer()
intensities = geometry.intensity_buffer()

target = geometry.center() + np.array([0.0, 0.0, 150.0])
option = LaguerreGaussianOption(p=0, l=1, waist=10.0)
wl = wavelength(340 * m / s)
laguerre_gaussian_phase(geometry, target, np.array([0.0, 0.0, 1.0]), option, wl, phases)
laguerre_gaussian_intensity(geometry, target, np.array([0.0, 0.0, 1.0]), option, wl, intensities)
