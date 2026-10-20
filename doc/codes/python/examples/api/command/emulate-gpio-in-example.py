import asyncio

from autd3 import Client, ClientConfig, UdpOption
from autd3.commands import EmulateGpioIn
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3()])
    async with await Client.open(geometry, UdpOption(), ClientConfig()) as client:
        await client.send(EmulateGpioIn([True, False, False, False]))


asyncio.run(main())
