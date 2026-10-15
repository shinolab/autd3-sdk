import asyncio

from autd3 import Client, ClientConfig, UdpEmulator
from autd3.commands import Pattern
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, s
from autd3.value import Intensity
from autd3_pattern import focus, wavelength


async def main() -> None:
    geometry = Geometry([Autd3()])
    emulator = UdpEmulator(geometry.num_devices())
    async with await Client.open(geometry, emulator.option(), ClientConfig()) as client:
        phases = geometry.phase_buffer()
        focus(
            geometry,
            geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm),
            wavelength(340 * m / s),
            phases,
        )

        await client.send(Pattern(phases, Intensity.MAX))


asyncio.run(main())
