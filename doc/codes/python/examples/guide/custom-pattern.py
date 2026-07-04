import numpy as np

import autd3_pattern as pattern
from autd3.commands import Pattern
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3.value import Emission, Intensity, Phase


def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    target = geometry.center() + np.array([0.0, 0.0, 150.0])
    wavelength = pattern.wavelength(340 * m / s)

    # ANCHOR: api
    emissions = geometry.pattern_buffer()
    for slot, device in zip(emissions, geometry):
        for t, pos in enumerate(device.positions()):
            dist = float(np.linalg.norm(target - pos))
            phase = round(-dist / wavelength * 256.0) & 0xFF
            slot[t] = Emission(
                Phase(phase),
                Intensity.MAX,
            )

    Pattern(emissions)
    # ANCHOR_END: api


if __name__ == "__main__":
    main()
