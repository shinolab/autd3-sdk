import asyncio

from autd3 import Client, ClientConfig, TransportOption
from autd3.commands import SetPulseWidthTable
from autd3.geometry import Autd3, Geometry
from autd3.value import PulseWidth


async def main() -> None:
    geometry = Geometry([Autd3()])
    async with await Client.open(geometry, TransportOption(), ClientConfig()) as client:
        table = SetPulseWidthTable.empty_table()
        for i in range(len(table)):
            table[i] = PulseWidth(i)

        await client.send(SetPulseWidthTable(table=table))


asyncio.run(main())
