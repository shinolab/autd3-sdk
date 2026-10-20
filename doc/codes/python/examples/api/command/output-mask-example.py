import asyncio

from autd3 import Client, ClientConfig, TransportOption
from autd3.commands import Clear, SetOutputMask, Synchronize
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3()])
    async with await Client.open(geometry, TransportOption(), ClientConfig()) as client:
        await client.send((Clear(), Synchronize()))

        masks = geometry.output_mask_buffer()

        await client.send(SetOutputMask(masks=masks))


asyncio.run(main())
