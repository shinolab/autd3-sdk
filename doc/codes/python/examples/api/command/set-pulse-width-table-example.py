import asyncio

from autd3 import Client, ClientConfig
from autd3.commands import SetPulseWidthTable
from autd3.geometry import Autd3, Geometry
from autd3_link_nop import Nop


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    client = await Client.open(geometry, Nop(), ClientConfig())

    table = SetPulseWidthTable.default_table()

    builder = client.datagram_builder()
    builder.push(SetPulseWidthTable(table=table))
    frames = builder.build()
    for frame in frames:
        await client.send_checked(frame)

    await client.close()


asyncio.run(main())
