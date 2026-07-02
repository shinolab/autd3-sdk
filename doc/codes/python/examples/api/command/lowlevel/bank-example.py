import asyncio

import numpy as np

from autd3 import Client, ClientConfig
from autd3.commands import ChangePatternBank, ConfigPattern, WritePatternBuffer
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3.value import LoopBehavior, PatternBank, SamplingConfig, TransitionMode
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

    bank = PatternBank.B0

    builder = client.datagram_builder()
    builder.push(
        WritePatternBuffer(
            bank=bank,
            index=0,
            emissions=emissions,
        )
    )
    builder.push(
        ConfigPattern(
            bank=bank,
            config=SamplingConfig(0xFFFF),
            size=1,
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
