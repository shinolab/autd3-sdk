import asyncio

from autd3 import Client, TransportOption
from autd3.commands import Clear, Pattern, Synchronize
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, s
from autd3.value import Intensity
from autd3_pattern import focus, wavelength


async def main() -> None:
    geometry = Geometry([Autd3()])
    with Client.open(geometry, TransportOption()) as client:
        await client.send(Clear())
        await client.send(Synchronize())

        phases = geometry.phase_buffer()
        focus(
            geometry,
            geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm),
            wavelength(340 * m / s),
            phases,
        )

        await client.send(Pattern(phases, Intensity.MAX))


asyncio.run(main())
