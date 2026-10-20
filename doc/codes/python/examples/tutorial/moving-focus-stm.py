import asyncio

from autd3 import Client, ClientConfig, TransportOption
from autd3.commands import Clear, FociStm, FociStmOption, Synchronize
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, mm
from autd3.value import ControlPoint, ControlPoints, Intensity, Phase

async def main() -> None:
    geometry = Geometry([Autd3()])

    async with await Client.open(
        geometry,
        TransportOption(),
        ClientConfig(),
    ) as client:
        await client.send((Clear(), Synchronize()))

        center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)

        # ANCHOR: stm
        points = [
            ControlPoints([ControlPoint(center + offset(20.0 * mm, 0.0 * mm, 0.0 * mm), Phase.ZERO)], Intensity.MAX),
            ControlPoints([ControlPoint(center + offset(-20.0 * mm, 0.0 * mm, 0.0 * mm), Phase.ZERO)], Intensity.MAX),
        ]
        await client.send(FociStm(0.5 * Hz, points, FociStmOption()))
        # ANCHOR_END: stm


if __name__ == "__main__":
    asyncio.run(main())
