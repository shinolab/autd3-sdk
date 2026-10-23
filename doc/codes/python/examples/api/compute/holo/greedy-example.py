import numpy as np
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, s
from autd3.value import Intensity
from autd3_pattern import TransducerMask, wavelength
from autd3_pattern_holo import (
    AmplitudeTarget,
    Directivity,
    GreedyOption,
    Pa,
    greedy,
)

geometry = Geometry([Autd3()])

order = np.random.default_rng().permutation(geometry.num_transducers())

phases = geometry.phase_buffer()

greedy(
    geometry,
    [
        AmplitudeTarget(
            point=geometry.center() + offset(-30.0 * mm, 0.0 * mm, 150.0 * mm),
            amplitude=2.5e3 * Pa,
        ),
        AmplitudeTarget(
            point=geometry.center() + offset(30.0 * mm, 0.0 * mm, 150.0 * mm),
            amplitude=2.5e3 * Pa,
        ),
    ],
    wavelength(340 * m / s),
    Intensity.MAX,
    order,
    GreedyOption(
        phase_quantization_levels=16,
        directivity=Directivity.Sphere,
        mask=TransducerMask.AllEnabled,
    ),
    phases,
)
