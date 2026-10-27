import asyncio

import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3 import Client, TransportOption
from autd3.commands import Clear, Pattern, SetSilencer, Synchronize
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, s
from autd3.value import Intensity

# xtask:long-running


async def main() -> None:
    geometry = Geometry([Autd3()])

    with Client.open(
        geometry,
        TransportOption(),
    ) as client:
        await client.send(Clear())
        await client.send(Synchronize())

        center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
        wavelength = pattern.wavelength(340 * m / s)

        # ANCHOR: loop
        phases = geometry.phase_buffer()
        while True:
            for sign in (1.0, -1.0):
                target = center + offset((sign * 20.0) * mm, 0.0 * mm, 0.0 * mm)
                pattern.focus(
                    geometry,
                    target,
                    wavelength,
                    phases,
                )
                await client.send(Pattern(phases, Intensity.MAX))
                await asyncio.sleep(1.0)
        # ANCHOR_END: loop


if __name__ == "__main__":
    asyncio.run(main())
