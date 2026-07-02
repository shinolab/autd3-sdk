import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3_pattern import wavelength
from autd3_pattern_holo import (
    ControlPoint,
    Directivity,
    EmissionConstraint,
    GreedyOption,
    Pa,
    TransducerMask,
    greedy,
)

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

center = geometry.center() + np.array([0.0, 0.0, 150.0])
foci = [
    ControlPoint(
        center + np.array([-30.0, 0.0, 0.0]),
        2.5e3 * Pa,
    ),
    ControlPoint(
        center + np.array([30.0, 0.0, 0.0]),
        2.5e3 * Pa,
    ),
]

wl = wavelength(340 * m / s)
option = (
    # ANCHOR: option
    GreedyOption(
        phase_quantization_levels=16,
        constraint=EmissionConstraint.Uniform(0xFF),
        directivity=Directivity.Sphere,
        mask=TransducerMask.AllEnabled,
    )
    # ANCHOR_END: option
)
out = geometry.pattern_buffer()
# ANCHOR: api
greedy(geometry, foci, wl, option, out)
# ANCHOR_END: api
