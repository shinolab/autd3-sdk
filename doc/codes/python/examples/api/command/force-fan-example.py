import asyncio

from autd3 import Client, ClientConfig
from autd3.commands import ForceFan
from autd3.geometry import Autd3, Geometry
from autd3_link_nop import Nop


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    client = await Client.open(geometry, Nop(), ClientConfig())

    builder = client.datagram_builder()
    builder.push(ForceFan(value=True))
    frames = builder.build()
    for frame in frames:
        await client.send_checked(frame)

    await client.close()


asyncio.run(main())
