import asyncio

from autd3 import Client, ClientConfig, TransportOption
from autd3.commands import Clear, SetPhaseCorrection, Synchronize
from autd3.geometry import Autd3, Geometry
from autd3.value import Phase


async def main() -> None:
    geometry = Geometry([Autd3()])
    async with await Client.open(geometry, TransportOption(), ClientConfig()) as client:
        await client.send((Clear(), Synchronize()))

        phases = [
            [Phase.ZERO] * geometry.device(i).num_transducers()
            for i in range(geometry.num_devices())
        ]

        await client.send(SetPhaseCorrection(phases=phases))


asyncio.run(main())
