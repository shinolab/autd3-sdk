from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, s
from autd3_pattern import wavelength as calc_wavelength
from autd3_pattern_holo import AmplitudeTarget, GsOption, Pa, gs_batch

geometry = Geometry([Autd3()])
center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
wavelength = calc_wavelength(340 * m / s)
option = GsOption()

# ANCHOR: api
problems = 64
foci = [
    AmplitudeTarget(
        center + offset(i * 0.5 * mm, 0.0 * mm, 0.0 * mm),
        5e3 * Pa,
    )
    for i in range(problems)
]

phases = [geometry.phase_buffer() for _ in range(problems)]
intensities = [geometry.intensity_buffer() for _ in range(problems)]
gs_batch(
    geometry,
    foci,
    wavelength,
    option,
    phases,
    intensities,
)
# ANCHOR_END: api
