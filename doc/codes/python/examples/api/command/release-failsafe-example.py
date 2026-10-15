import asyncio

from autd3 import Client, ClientConfig, UdpEmulator
from autd3.commands import ReleaseFailsafe
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3()])
    emulator = UdpEmulator(geometry.num_devices())
    async with await Client.open(geometry, emulator.option(), ClientConfig()) as client:
        states = await client.read_fpga_state()
        if any(state.is_failsafe_active() for state in states):
            await client.send(ReleaseFailsafe())


asyncio.run(main())
