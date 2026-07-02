import numpy as np

import autd3
import autd3_pattern as pattern
from autd3.units import m, s


def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    target = geometry.center() + np.array([0.0, 0.0, 150.0])
    wavelength = pattern.wavelength(340 * m / s)

    # ANCHOR: api
    emissions = []
    for device in geometry:
        slot = []
        for pos in device.positions():
            dist = float(np.linalg.norm(target - pos))
            phase = round(-dist / wavelength * 256.0) & 0xFF
            slot.append(
                autd3.value.Emission(
                    autd3.value.Phase(phase),
                    autd3.value.Intensity.MAX,
                )
            )
        emissions.append(slot)
    buffer = pattern.PatternBuffer.from_array(emissions)

    autd3.commands.Pattern(buffer)
    # ANCHOR_END: api


if __name__ == "__main__":
    main()
