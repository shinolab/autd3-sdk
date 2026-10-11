import asyncio

import numpy as np

from autd3 import Client, ClientConfig, UdpEmulator
from autd3.commands import FociStm, FociStmOption, circle
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

        await client.send(
            FociStm(
                1.0 * Hz,
                points,
                FociStmOption(
                    bank=PatternBank.B0,
                    sound_speed=340 * m / s,
                    loop_behavior=LoopBehavior.Infinite,
                    transition_mode=TransitionMode.Immediate,
                ),
            )
        )


asyncio.run(main())
