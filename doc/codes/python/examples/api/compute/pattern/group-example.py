from enum import Enum, auto

import numpy as np

from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3_pattern import (
    FocusOption,
    PatternBuffer,
    TransducerGroups,
    TransducerMask,
    focus,
    group_compute,
    wavelength,
)
from autd3_pattern_holo import AmplitudeTarget, GspatOption, Pa, gspat


class Side(Enum):
    LEFT = auto()
    RIGHT = auto()


geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
wl = wavelength(340 * m / s)
center = geometry.center()

groups = TransducerGroups(
    geometry,
    lambda device, tr: Side.LEFT if device.position(tr)[0] < center[0] else Side.RIGHT,
)

foci = [
    AmplitudeTarget(point=center + np.array([-50.0, 0.0, 150.0]), amplitude=5e3 * Pa),
    AmplitudeTarget(point=center + np.array([-20.0, 0.0, 150.0]), amplitude=5e3 * Pa),
]


def compute(side: Side, mask: TransducerMask, buffer: PatternBuffer) -> None:
    if side is Side.LEFT:
        gspat(geometry, foci, wl, GspatOption(mask=mask), buffer)
    else:
        focus(geometry, center + np.array([40.0, 0.0, 150.0]), wl, FocusOption(), buffer)


dst = geometry.pattern_buffer()
group_compute(geometry, groups, compute, dst)
