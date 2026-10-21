import asyncio
import math

from autd3 import Client, TransportOption
from autd3.commands import Clear, FociStm, FociStmOption, Synchronize
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, m, mm, s
from autd3.value import ControlPoint, ControlPoints, Intensity, LoopBehavior, PatternBank, Phase, SamplingConfig, TransitionMode


async def main() -> None:
    geometry = Geometry([Autd3()])
    async with Client.open(geometry, TransportOption()) as client:
        await client.send((Clear(), Synchronize()))

        center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
        points = []
        for i in range(200):
            theta = 2.0 * math.pi * i / 200
            p = center + offset(30.0 * mm * math.cos(theta), 30.0 * mm * math.sin(theta), 0.0 * mm)
            points.append(ControlPoints([ControlPoint(p, Phase.ZERO)], Intensity.MAX))

        await client.send(
            FociStm(
                SamplingConfig(1.0 * Hz * len(points)),
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
