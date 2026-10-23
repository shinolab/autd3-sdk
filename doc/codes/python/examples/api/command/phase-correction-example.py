import asyncio

from autd3 import Client, TransportOption
from autd3.commands import Clear, SetPhaseCorrection, Synchronize
from autd3.geometry import Autd3, Geometry
from autd3.value import Phase


async def main() -> None:
    geometry = Geometry([Autd3()])
    with Client.open(geometry, TransportOption()) as client:
        await client.send((Clear(), Synchronize()))

        phases = [[Phase.ZERO] * device.num_transducers() for device in geometry]

        await client.send(SetPhaseCorrection(phases=phases))


asyncio.run(main())
