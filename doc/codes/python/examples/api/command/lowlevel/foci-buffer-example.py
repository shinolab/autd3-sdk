import asyncio

import numpy as np

from autd3 import Client, ClientConfig, UdpEmulator
from autd3.commands import ActivatePatternBank, ConfigFociStm, StmConfig, WriteFociBuffer, circle
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz, m, mm, s
from autd3.value import Intensity, LoopBehavior, PatternBank, TransitionMode


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    emulator = UdpEmulator(geometry.num_devices())
    async with await Client.open(geometry, emulator.option(), ClientConfig()) as client:
        center = geometry.center() + np.array([0.0, 0.0, 150.0])
        points = []
        circle(center, 30.0 * mm, 200, [0.0, 0.0, 1.0], Intensity.MAX, points)

        bank = PatternBank.B0

        await client.send(
            WriteFociBuffer(
                bank=bank,
                index_offset=0,
                points=points,
            )
        )
        await client.send(
            ConfigFociStm(
                bank=bank,
                config=StmConfig(1.0 * Hz).into_sampling_config(len(points)),
                size=len(points),
                num_foci=1,
                sound_speed=340.0 * m / s,
                loop_behavior=LoopBehavior.Infinite,
            )
        )
        await client.send(
            ActivatePatternBank(
                bank=bank,
                transition_mode=TransitionMode.Immediate,
            )
        )


asyncio.run(main())
