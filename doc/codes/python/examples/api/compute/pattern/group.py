from enum import Enum, auto

import numpy as np

from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
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
    wavelength,
)
from autd3_pattern_holo import AmplitudeTarget, GspatOption, Pa, gspat


# ANCHOR: api
# ANCHOR: compute
class Side(Enum):
    LEFT = auto()
    RIGHT = auto()
# ANCHOR_END: compute
# ANCHOR_END: api


geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

left = geometry.phase_buffer()
right = geometry.phase_buffer()
set_phase(Phase(0x80), right)
dst = geometry.phase_buffer()
center = geometry.center()

# ANCHOR: api
groups = TransducerGroups(
    geometry,
    lambda device, tr: Side.LEFT if device.position(tr)[0] < center[0] else Side.RIGHT,
)
group(geometry, groups, {Side.LEFT: left, Side.RIGHT: right}, dst)
# ANCHOR_END: api

wl = wavelength(340 * m / s)
foci = [AmplitudeTarget(point=center + np.array([-30.0, 0.0, 150.0]), amplitude=5e3 * Pa)]
target = center + np.array([40.0, 0.0, 150.0])
phases = geometry.phase_buffer()
intensities = geometry.intensity_buffer()


# ANCHOR: compute
def compute(side: Side, mask: TransducerMask, phases: PhaseBuffer, intensities: IntensityBuffer) -> None:
    if side is Side.LEFT:
        gspat(geometry, foci, wl, GspatOption(mask=mask), phases, intensities)
    else:
        focus(geometry, target, wl, phases)


group_compute(geometry, groups, compute, phases, intensities)
# ANCHOR_END: compute
