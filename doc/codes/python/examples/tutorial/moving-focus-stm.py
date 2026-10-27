import asyncio

from autd3 import Client, TransportOption
from autd3.commands import Clear, FociStm, FociStmOption, Pattern, Synchronize
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, mm
from autd3.value import ControlPoints, Intensity, SamplingConfig

async def main() -> None:
    geometry = Geometry([Autd3()])

    with Client.open(
        geometry,
        TransportOption(),
    ) as client:
        await client.send(Clear())
        await client.send(Synchronize())

        center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)

        # ANCHOR: stm
        points = [
            ControlPoints(center + offset(20.0 * mm, 0.0 * mm, 0.0 * mm)),
            ControlPoints(center + offset(-20.0 * mm, 0.0 * mm, 0.0 * mm)),
        ]
        config = SamplingConfig(0.5 * Hz * len(points))
        await client.send(FociStm(config, points, FociStmOption()))
        # ANCHOR_END: stm

        await client.send(Pattern(geometry.phase_buffer(), Intensity.MIN))
        client.close()


if __name__ == "__main__":
    asyncio.run(main())
