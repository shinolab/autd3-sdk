import asyncio

from autd3 import Client, TransportOption
from autd3.commands import Clear, SetPhaseCorrection, Synchronize
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3()])
    with Client.open(geometry, TransportOption()) as client:
        await client.send(Clear())
        await client.send(Synchronize())

        phases = geometry.phase_buffer()

        await client.send(SetPhaseCorrection(phases))


asyncio.run(main())
