from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, s
from autd3_pattern import focus, wavelength as calc_wavelength

geometry = Geometry([Autd3()])

target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
wavelength = calc_wavelength(340 * m / s)
phases = geometry.phase_buffer()

# ANCHOR: api
focus(geometry, target, wavelength, phases)
# ANCHOR_END: api
