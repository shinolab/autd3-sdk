import math

import numpy as np

import autd3_pattern as pattern
from autd3.commands import Pattern
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, rad, s
from autd3.value import Intensity, Phase


def main() -> None:
    geometry = Geometry([Autd3()])

    target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
    wavelength = pattern.wavelength(340 * m / s)

    # ANCHOR: api
    phases = geometry.phase_buffer()
    for slot, device in zip(phases, geometry):
        for t, pos in enumerate(device.positions()):
            dist = float(np.linalg.norm(target - pos))
            slot[t] = Phase(-dist / wavelength.mm * 2.0 * math.pi * rad)

    Pattern(phases, Intensity.MAX)
    # ANCHOR_END: api


if __name__ == "__main__":
    main()
