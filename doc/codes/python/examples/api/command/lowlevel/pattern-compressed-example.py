import asyncio

import numpy as np

from autd3 import Client, ClientConfig
from autd3.commands import ChangePatternBank, ConfigPattern, PatternCompression, StmConfig, WritePatternCompressed
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz, m, s
from autd3.value import LoopBehavior, PatternBank, TransitionMode
from autd3_link_nop import Nop
from autd3_pattern import FocusOption, focus, wavelength


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    client = await Client.open(geometry, Nop(), ClientConfig())

    center = geometry.center() + np.array([0.0, 0.0, 150.0])
    wl = wavelength(340 * m / s)
    patterns = []
    for x in (-30.0, -10.0, 10.0, 30.0):
        buffer = geometry.pattern_buffer()
        focus(
            geometry,
            center + np.array([x, 0.0, 0.0]),
            wl,
            FocusOption(),
            buffer,
        )
        patterns.append(buffer)

    bank = PatternBank.B0

    builder = client.datagram_builder()
    builder.push(
        WritePatternCompressed(
            bank=bank,
            index=0,
            format=PatternCompression.PhaseHalf,
            patterns=patterns,
        )
    )
    builder.push(
        ConfigPattern(
            bank=bank,
            config=StmConfig(1.0 * Hz).into_sampling_config(len(patterns)),
            size=len(patterns),
            loop_behavior=LoopBehavior.Infinite,
        )
    )
    builder.push(
        ChangePatternBank(
            bank=bank,
            transition_mode=TransitionMode.Immediate,
        )
    )
    frames = builder.build()
    for frame in frames:
        await client.send_checked(frame)

    await client.close()


asyncio.run(main())
