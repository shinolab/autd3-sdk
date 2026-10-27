import asyncio

import autd3_pattern as pattern
from autd3 import Client, TransportOption
from autd3.commands import Clear, Pattern, Synchronize
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, s
from autd3.value import Intensity, PatternBank

async def main() -> None:
    geometry = Geometry([Autd3()])

    with Client.open(
        geometry,
        TransportOption(),
    ) as client:
        await client.send(Clear())
        await client.send(Synchronize())

        wavelength = pattern.wavelength(340 * m / s)


        # ANCHOR: switch
        # Write focus A to bank B0 and play it.
        target_a = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
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
        target_b = geometry.center() + offset(0.0 * mm, 30.0 * mm, 150.0 * mm)
        pat_b = geometry.phase_buffer()
        pattern.focus(
            geometry,
            target_b,
            wavelength,
            pat_b,
        )
        await client.send(Pattern(pat_b, Intensity.MAX, bank=PatternBank.B1))
        # ANCHOR_END: switch

        await client.send(Pattern(geometry.phase_buffer(), Intensity.MIN))
        client.close()


if __name__ == "__main__":
    asyncio.run(main())
