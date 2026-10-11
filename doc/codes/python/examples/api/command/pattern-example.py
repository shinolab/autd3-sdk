import asyncio

import numpy as np

from autd3 import Client, ClientConfig, UdpEmulator
from autd3.commands import Pattern
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3.value import Intensity
from autd3_pattern import focus, wavelength


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    emulator = UdpEmulator(geometry.num_devices())
    async with await Client.open(geometry, emulator.option(), ClientConfig()) as client:
        phases = geometry.phase_buffer()
        focus(
            geometry,
            geometry.center() + np.array([0.0, 0.0, 150.0]),
            wavelength(340 * m / s),
            phases,
        )

        await client.send(Pattern(phases, Intensity.MAX))


asyncio.run(main())
