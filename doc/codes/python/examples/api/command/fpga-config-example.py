import asyncio

from autd3 import Client, TransportOption
from autd3.commands import Clear, FpgaConfig, SetFpgaConfig, Synchronize
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3()])
    async with Client.open(geometry, TransportOption()) as client:
        await client.send((Clear(), Synchronize()))

        await client.send(SetFpgaConfig(FpgaConfig(sync_phase_lock=False)))

        await client.send(SetFpgaConfig())


asyncio.run(main())
