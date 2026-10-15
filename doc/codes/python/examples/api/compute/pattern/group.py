from enum import Enum, auto

from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, s
from autd3.value import Phase
from autd3_pattern import (
    IntensityBuffer,
    PhaseBuffer,
    TransducerGroups,
    TransducerMask,
    focus,
    group,
    group_compute,
    set_phase,
)
from autd3_pattern import wavelength as calc_wavelength
from autd3_pattern_holo import AmplitudeTarget, GspatOption, Pa, gspat


# ANCHOR: api
# ANCHOR: compute
class Side(Enum):
    LEFT = auto()
    RIGHT = auto()
# ANCHOR_END: compute
# ANCHOR_END: api


geometry = Geometry([Autd3()])

left = geometry.phase_buffer()
right = geometry.phase_buffer()
set_phase(Phase.PI, right)
dst = geometry.phase_buffer()
center = geometry.center()

# ANCHOR: api
groups = TransducerGroups(
    geometry,
    lambda device, tr: Side.LEFT if device.position(tr)[0] < center[0] else Side.RIGHT,
)
group(geometry, groups, {Side.LEFT: left, Side.RIGHT: right}, dst)
# ANCHOR_END: api

wavelength = calc_wavelength(340 * m / s)
foci = [AmplitudeTarget(point=center + offset(-30.0 * mm, 0.0 * mm, 150.0 * mm), amplitude=5e3 * Pa)]
target = center + offset(40.0 * mm, 0.0 * mm, 150.0 * mm)
phases = geometry.phase_buffer()
intensities = geometry.intensity_buffer()


# ANCHOR: compute
def compute(side: Side, mask: TransducerMask, phases: PhaseBuffer, intensities: IntensityBuffer) -> None:
    if side is Side.LEFT:
        gspat(geometry, foci, wavelength, GspatOption(mask=mask), phases, intensities)
    else:
        focus(geometry, target, wavelength, phases)


group_compute(geometry, groups, compute, phases, intensities)
# ANCHOR_END: compute
