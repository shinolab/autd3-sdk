import asyncio

from autd3 import Client, Duration, TransportOption
from autd3.commands import Clear, CpuConfig, SetCpuConfig, Synchronize
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3()])
    with Client.open(geometry, TransportOption()) as client:
        await client.send(Clear())
        await client.send(Synchronize())

        await client.send(SetCpuConfig(CpuConfig(ptp_unlock_failsafe_timeout=Duration.from_secs(2))))


asyncio.run(main())
