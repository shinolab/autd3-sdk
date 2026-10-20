import asyncio

from autd3 import Client, TransportOption
from autd3.commands import Synchronize
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3()])
    async with Client.open(geometry, TransportOption()) as client:
        await client.send(Synchronize())


asyncio.run(main())
