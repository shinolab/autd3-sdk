from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, s
from autd3.value import Intensity
from autd3_pattern import TransducerMask
from autd3_pattern import wavelength as calc_wavelength
from autd3_pattern_holo import (
    AmplitudeTarget,
    Directivity,
    IntensityConstraint,
    NaiveOption,
    Pa,
    naive,
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
constraint = IntensityConstraint.Clamp(Intensity.MIN, Intensity.MAX)
directivity = Directivity.Sphere
mask = TransducerMask.AllEnabled
option = (
    # ANCHOR: option
    NaiveOption(
        constraint,
        directivity,
        mask,
    )
    # ANCHOR_END: option
)
phases = geometry.phase_buffer()
intensities = geometry.intensity_buffer()
# ANCHOR: api
naive(geometry, foci, wavelength, option, phases, intensities)
# ANCHOR_END: api
