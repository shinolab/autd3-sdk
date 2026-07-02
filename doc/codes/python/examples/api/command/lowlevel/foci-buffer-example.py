import asyncio

import numpy as np

from autd3 import Client, ClientConfig
from autd3.commands import ChangePatternBank, ConfigFociStm, StmConfig, WriteFociBuffer, circle
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz, m, s
from autd3.value import Intensity, LoopBehavior, PatternBank, TransitionMode
from autd3_link_nop import Nop


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    client = await Client.open(geometry, Nop(), ClientConfig())

    center = geometry.center() + np.array([0.0, 0.0, 150.0])
    points = []
    circle(center, 30.0, 200, [0.0, 0.0, 1.0], Intensity.MAX, points)

    bank = PatternBank.B0

    builder = client.datagram_builder()
    builder.push(
        WriteFociBuffer(
            bank=bank,
            index_offset=0,
            points=points,
        )
    )
    builder.push(
        ConfigFociStm(
            bank=bank,
            config=StmConfig(1.0 * Hz).into_sampling_config(len(points)),
            size=len(points),
            num_foci=1,
            sound_speed=340.0 * m / s,
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
