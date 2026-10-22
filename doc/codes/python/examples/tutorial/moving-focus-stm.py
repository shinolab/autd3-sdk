import asyncio

from autd3 import Client, TransportOption
from autd3.commands import Clear, FociStm, FociStmOption, Pattern, Synchronize
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, mm
from autd3.value import ControlPoint, ControlPoints, Intensity, Phase, SamplingConfig

async def main() -> None:
    geometry = Geometry([Autd3()])

    with Client.open(
        geometry,
        TransportOption(),
    ) as client:
        await client.send((Clear(), Synchronize()))

        center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)

        # ANCHOR: stm
        points = [
            ControlPoints([ControlPoint(center + offset(20.0 * mm, 0.0 * mm, 0.0 * mm), Phase.ZERO)], Intensity.MAX),
            ControlPoints([ControlPoint(center + offset(-20.0 * mm, 0.0 * mm, 0.0 * mm), Phase.ZERO)], Intensity.MAX),
        ]
        config = SamplingConfig(0.5 * Hz * len(points))
        await client.send(FociStm(config, points, FociStmOption()))
        # ANCHOR_END: stm

        await client.send(Pattern(geometry.phase_buffer(), Intensity.MIN))


if __name__ == "__main__":
    asyncio.run(main())
