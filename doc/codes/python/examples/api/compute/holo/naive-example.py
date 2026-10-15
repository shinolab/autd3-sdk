from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, s
from autd3.value import Intensity
from autd3_pattern import TransducerMask, wavelength
from autd3_pattern_holo import (
    AmplitudeTarget,
    Directivity,
    IntensityConstraint,
    NaiveOption,
    Pa,
    naive,
)

geometry = Geometry([Autd3()])

phases = geometry.phase_buffer()
intensities = geometry.intensity_buffer()

naive(
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
    NaiveOption(
        constraint=IntensityConstraint.Clamp(Intensity.MIN, Intensity.MAX),
        directivity=Directivity.Sphere,
        mask=TransducerMask.AllEnabled,
        parallel=True,
    ),
    phases,
    intensities,
)
