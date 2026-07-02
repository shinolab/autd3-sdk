import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3_pattern import wavelength
from autd3_pattern_holo import (
    ControlPoint,
    Directivity,
    EmissionConstraint,
    NaiveOption,
    NalgebraBackend,
    Pa,
    TransducerMask,
    naive,
)

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

out = geometry.pattern_buffer()

naive(
    geometry,
    [
        ControlPoint(
            point=geometry.center() + np.array([-30.0, 0.0, 150.0]),
            amplitude=2.5e3 * Pa,
        ),
        ControlPoint(
            point=geometry.center() + np.array([30.0, 0.0, 150.0]),
            amplitude=2.5e3 * Pa,
        ),
    ],
    wavelength(340 * m / s),
    NaiveOption(
        constraint=EmissionConstraint.Clamp(0x00, 0xFF),
        directivity=Directivity.Sphere,
        backend=NalgebraBackend(),
        mask=TransducerMask.AllEnabled,
    ),
    out,
)
