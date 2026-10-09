import asyncio

from autd3 import Client, ClientConfig, UdpEmulator
from autd3.commands import SetOutputMask
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    emulator = UdpEmulator(geometry.num_devices())
    async with await Client.open(geometry, emulator.option(), ClientConfig()) as client:
        masks = [
            [True] * geometry.device(i).num_transducers()
            for i in range(geometry.num_devices())
        ]

        await client.send(SetOutputMask(masks=masks))


asyncio.run(main())
