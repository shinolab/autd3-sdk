import asyncio
import math

from autd3 import Client, TransportOption
from autd3.commands import ActivatePatternBank, Clear, ConfigFociStm, Synchronize, WriteFociBuffer
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

        bank = PatternBank.B0

        max_len = WriteFociBuffer.max_len(1)
        for start_index in range(0, len(points), max_len):
            await client.send(
                WriteFociBuffer(
                    bank=bank,
                    start_index=start_index,
                    points=points[start_index : start_index + max_len],
                )
            )
        await client.send(
            ConfigFociStm(
                bank=bank,
                config=SamplingConfig(1.0 * Hz * len(points)),
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
