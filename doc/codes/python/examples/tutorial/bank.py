import asyncio

import numpy as np

import autd3_pattern as pattern
from autd3 import Client, ClientConfig, TransportOption
from autd3.commands import Pattern
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3.value import Intensity, PatternBank

async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    async with await Client.open(
        geometry,
        TransportOption(),
        ClientConfig(),
    ) as client:
        wavelength = pattern.wavelength(340 * m / s)


        # ANCHOR: switch
        # Write focus A to bank B0 and play it.
        target_a = geometry.center() + np.array([0.0, 0.0, 150.0])
        pat_a = geometry.phase_buffer()
        pattern.focus(
            geometry,
            target_a,
            wavelength,
            pat_a,
        )
        await client.send(Pattern(pat_a, Intensity.MAX, bank=PatternBank.B0))

        # Write focus B to bank B1, which is not currently playing, then switch to B1.
        # B0 keeps playing cleanly while B1 is being written (double buffering).
        target_b = geometry.center() + np.array([0.0, 30.0, 150.0])
        pat_b = geometry.phase_buffer()
        pattern.focus(
            geometry,
            target_b,
            wavelength,
            pat_b,
        )
        await client.send(Pattern(pat_b, Intensity.MAX, bank=PatternBank.B1))
        # ANCHOR_END: switch


if __name__ == "__main__":
    asyncio.run(main())
