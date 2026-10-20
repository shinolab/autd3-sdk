import asyncio

from autd3 import Client, TransportOption
from autd3.commands import Clear, SetPulseWidthTable, Synchronize
from autd3.geometry import Autd3, Geometry
from autd3.value import PulseWidth


async def main() -> None:
    geometry = Geometry([Autd3()])
    async with Client.open(geometry, TransportOption()) as client:
        await client.send((Clear(), Synchronize()))

        table = SetPulseWidthTable.empty_table()
        for i in range(len(table)):
            table[i] = PulseWidth(i)

        await client.send(SetPulseWidthTable(table=table))


asyncio.run(main())
