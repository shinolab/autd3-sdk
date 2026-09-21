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
    GreedyOption,
    Pa,
    greedy,
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
phase_quantization_levels = 16
constraint = IntensityConstraint.Uniform(Intensity.MAX)
directivity = Directivity.Sphere
mask = TransducerMask.AllEnabled
option = (
    # ANCHOR: option
    GreedyOption(
        phase_quantization_levels,
        constraint,
        directivity,
        mask,
    )
    # ANCHOR_END: option
)
phases = geometry.phase_buffer()
intensities = geometry.intensity_buffer()
# ANCHOR: api
greedy(geometry, foci, wavelength, option, phases, intensities)
# ANCHOR_END: api
