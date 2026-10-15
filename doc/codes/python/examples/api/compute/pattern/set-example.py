from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, s
from autd3.value import Intensity, Phase
from autd3_pattern import add_phase, focus, set_intensity, wavelength

geometry = Geometry([Autd3()])

phases = geometry.phase_buffer()
intensities = geometry.intensity_buffer()

set_intensity(Intensity(0x80), intensities)
focus(
    geometry,
    geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm),
    wavelength(340 * m / s),
    phases,
)
add_phase(Phase.PI, phases)

set_intensity(Intensity.MIN, intensities)
