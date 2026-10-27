import asyncio

from autd3 import Client, TransportOption
from autd3.commands import Clear, ReleaseFailsafe, Synchronize
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3()])
    with Client.open(geometry, TransportOption()) as client:
        await client.send(Clear())
        await client.send(Synchronize())

        states = await client.read_fpga_state()
        if any(state.is_failsafe_active() for state in states):
            await client.send(ReleaseFailsafe())


asyncio.run(main())
