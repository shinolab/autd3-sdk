import asyncio

import numpy as np

import autd3_link_nop as nop
import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3 import Client, ClientConfig
from autd3.commands import Pattern, SetSilencer
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz, m, s

async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]), Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    client = await Client.open(geometry, nop.Nop(), ClientConfig())

    # ANCHOR: api
    builder = client.datagram_builder()
    builder.push(SetSilencer())
    frames = builder.build()
    for frame in frames:
        await client.send_checked(frame)
    # ANCHOR_END: api

    wavelength = pattern.wavelength(340 * m / s)
    left = geometry.pattern_buffer()
    pattern.focus(geometry, geometry.center() + np.array([-40.0, 0.0, 150.0]), wavelength, pattern.FocusOption(), left)
    right = geometry.pattern_buffer()
    pattern.focus(geometry, geometry.center() + np.array([40.0, 0.0, 150.0]), wavelength, pattern.FocusOption(), right)

    # ANCHOR: push_each
    builder = client.datagram_builder()
    builder.push_each(lambda device: Pattern(left if device % 2 == 0 else right))
    frames = builder.build()
    # ANCHOR_END: push_each

    for frame in frames:
        await client.send_checked(frame)

    await client.close()


asyncio.run(main())
