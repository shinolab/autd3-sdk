from enum import Enum, auto

from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, s
from autd3_pattern import (
    IntensityBuffer,
    PhaseBuffer,
    TransducerGroups,
    TransducerMask,
    focus,
    group_compute,
)
from autd3_pattern import wavelength as calc_wavelength
from autd3_pattern_holo import AmplitudeTarget, GspatOption, Pa, gspat


class Side(Enum):
    LEFT = auto()
    RIGHT = auto()


geometry = Geometry([Autd3()])
wavelength = calc_wavelength(340 * m / s)
center = geometry.center()

groups = TransducerGroups(
    geometry,
    lambda device, tr: Side.LEFT if device.position(tr)[0] < center[0] else Side.RIGHT,
)

foci = [
    AmplitudeTarget(point=center + offset(-50.0 * mm, 0.0 * mm, 150.0 * mm), amplitude=5e3 * Pa),
    AmplitudeTarget(point=center + offset(-20.0 * mm, 0.0 * mm, 150.0 * mm), amplitude=5e3 * Pa),
]


def compute(side: Side, mask: TransducerMask, phases: PhaseBuffer, intensities: IntensityBuffer) -> None:
    if side is Side.LEFT:
        gspat(geometry, foci, wavelength, GspatOption(mask=mask), phases, intensities)
    else:
        focus(geometry, center + offset(40.0 * mm, 0.0 * mm, 150.0 * mm), wavelength, phases)


phases = geometry.phase_buffer()
intensities = geometry.intensity_buffer()
group_compute(geometry, groups, compute, phases, intensities)
