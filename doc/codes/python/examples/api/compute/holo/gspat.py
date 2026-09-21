import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3.value import Intensity
from autd3_pattern import TransducerMask
from autd3_pattern import wavelength as calc_wavelength
from autd3_pattern_holo import (
    AmplitudeTarget,
    Directivity,
    IntensityConstraint,
    GspatOption,
    Pa,
    gspat,
)

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

center = geometry.center() + np.array([0.0, 0.0, 150.0])
foci = [
    AmplitudeTarget(
        center + np.array([-30.0, 0.0, 0.0]),
        2.5e3 * Pa,
    ),
    AmplitudeTarget(
        center + np.array([30.0, 0.0, 0.0]),
        2.5e3 * Pa,
    ),
]

wavelength = calc_wavelength(340 * m / s)
repeat = 100
constraint = IntensityConstraint.Clamp(Intensity.MIN, Intensity.MAX)
directivity = Directivity.Sphere
mask = TransducerMask.AllEnabled
parallel = True
option = (
    # ANCHOR: option
    GspatOption(
        repeat,
        constraint,
        directivity,
        mask,
        parallel,
    )
    # ANCHOR_END: option
)
phases = geometry.phase_buffer()
intensities = geometry.intensity_buffer()
# ANCHOR: api
gspat(geometry, foci, wavelength, option, phases, intensities)
# ANCHOR_END: api
