import asyncio

import numpy as np

from autd3 import Client, ClientConfig
from autd3.commands import Pattern
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3_link_nop import Nop
from autd3_pattern import FocusOption, focus, wavelength


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    client = await Client.open(geometry, Nop(), ClientConfig())

    emissions = geometry.pattern_buffer()
    focus(
        geometry,
        geometry.center() + np.array([0.0, 0.0, 150.0]),
        wavelength(340 * m / s),
        FocusOption(),
        emissions,
    )

    builder = client.datagram_builder()
    builder.push(Pattern(emissions))
    frames = builder.build()
    for frame in frames:
        await client.send_checked(frame)

    await client.close()


asyncio.run(main())
