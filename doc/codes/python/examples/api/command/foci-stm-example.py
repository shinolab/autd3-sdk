import asyncio

from autd3 import Client, ClientConfig, UdpOption
from autd3.commands import FociStm, FociStmOption, circle
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, m, mm, s
from autd3.value import Intensity, LoopBehavior, PatternBank, TransitionMode


async def main() -> None:
    geometry = Geometry([Autd3()])
    async with await Client.open(geometry, UdpOption(), ClientConfig()) as client:
        center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
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
