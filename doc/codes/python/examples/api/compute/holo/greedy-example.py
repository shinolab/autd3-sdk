import numpy as np
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3_pattern import TransducerMask, wavelength
from autd3_pattern_holo import (
    AmplitudeTarget,
    Directivity,
    IntensityConstraint,
    GreedyOption,
    Pa,
    greedy,
)

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

phases = geometry.phase_buffer()
intensities = geometry.intensity_buffer()

greedy(
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
    GreedyOption(
        phase_quantization_levels=16,
        constraint=IntensityConstraint.Uniform(0xFF),
        directivity=Directivity.Sphere,
        mask=TransducerMask.AllEnabled,
    ),
    phases,
    intensities,
)
