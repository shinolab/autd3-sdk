import numpy as np
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, s
from autd3_pattern import wavelength as calc_wavelength
from autd3_pattern_holo import (
    AmplitudeTarget,
    Directivity,
    GreedyOption,
    Pa,
    greedy,
)

geometry = Geometry([Autd3()])

center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
foci = [
    AmplitudeTarget(
        center + offset(-30.0 * mm, 0.0 * mm, 0.0 * mm),
        2.5e3 * Pa,
    ),
    AmplitudeTarget(
        center + offset(30.0 * mm, 0.0 * mm, 0.0 * mm),
        2.5e3 * Pa,
    ),
]

wavelength = calc_wavelength(340 * m / s)
phase_quantization_levels = 16
directivity = Directivity.Sphere
option = (
    # ANCHOR: option
    GreedyOption(
        phase_quantization_levels,
        directivity,
    )
    # ANCHOR_END: option
)
intensities = geometry.intensity_buffer()
order = np.random.default_rng().permutation(geometry.num_transducers())
dst = geometry.phase_buffer()
# ANCHOR: api
greedy(geometry, foci, wavelength, intensities, order, option, dst)
# ANCHOR_END: api
