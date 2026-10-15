from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, s
from autd3_pattern import focus, wavelength

geometry = Geometry([Autd3()])

phases = geometry.phase_buffer()

focus(
    geometry,
    geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm),
    wavelength(340 * m / s),
    phases,
)
