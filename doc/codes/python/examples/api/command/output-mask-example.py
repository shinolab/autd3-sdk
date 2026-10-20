import asyncio

from autd3 import Client, ClientConfig, UdpOption
from autd3.commands import SetOutputMask
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3()])
    async with await Client.open(geometry, UdpOption(), ClientConfig()) as client:
        masks = geometry.output_mask_buffer()

        await client.send(SetOutputMask(masks=masks))


asyncio.run(main())
