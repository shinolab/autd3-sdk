import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3_pattern import wavelength
from autd3_pattern_holo import (
    AmplitudeTarget,
    Directivity,
    EmissionConstraint,
    GspatOption,
    Pa,
    TransducerMask,
    gspat,
)

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

dst = geometry.pattern_buffer()

gspat(
    geometry,
    [
        AmplitudeTarget(
            point=geometry.center() + np.array([-30.0, 0.0, 150.0]),
            amplitude=2.5e3 * Pa,
        ),
        AmplitudeTarget(
            point=geometry.center() + np.array([30.0, 0.0, 150.0]),
            amplitude=2.5e3 * Pa,
        ),
    ],
    wavelength(340 * m / s),
    GspatOption(
        repeat=100,
        constraint=EmissionConstraint.Clamp(0x00, 0xFF),
        directivity=Directivity.Sphere,
        mask=TransducerMask.AllEnabled,
    ),
    dst,
)
