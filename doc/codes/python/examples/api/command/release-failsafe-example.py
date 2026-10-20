import asyncio

from autd3 import Client, ClientConfig, UdpOption
from autd3.commands import ReleaseFailsafe
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3()])
    async with await Client.open(geometry, UdpOption(), ClientConfig()) as client:
        states = await client.read_fpga_state()
        if any(state.is_failsafe_active() for state in states):
            await client.send(ReleaseFailsafe())


asyncio.run(main())
