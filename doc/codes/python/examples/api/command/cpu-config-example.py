import asyncio

from autd3 import Client, Duration, TransportOption
from autd3.commands import Clear, CpuConfig, SetCpuConfig, Synchronize
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3()])
    async with Client.open(geometry, TransportOption()) as client:
        await client.send((Clear(), Synchronize()))

        await client.send(SetCpuConfig(CpuConfig(sys_time_transition_margin=Duration.from_millis(20))))


asyncio.run(main())
