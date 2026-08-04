import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3_pattern import wavelength as calc_wavelength
from autd3_pattern_holo import (
    AmplitudeTarget,
    Directivity,
    EmissionConstraint,
    NaiveOption,
    Pa,
    TransducerMask,
    naive,
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
constraint = EmissionConstraint.Clamp(0x00, 0xFF)
directivity = Directivity.Sphere
mask = TransducerMask.AllEnabled
parallel = True
option = (
    # ANCHOR: option
    NaiveOption(
        constraint,
        directivity,
        mask,
        parallel,
    )
    # ANCHOR_END: option
)
dst = geometry.pattern_buffer()
# ANCHOR: api
naive(geometry, foci, wavelength, option, dst)
# ANCHOR_END: api
