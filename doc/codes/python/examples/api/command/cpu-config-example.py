import asyncio

from autd3 import Client, ClientConfig, Duration, UdpEmulator
from autd3.commands import CpuConfig, SetCpuConfig
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    emulator = UdpEmulator(geometry.num_devices())
    async with await Client.open(geometry, emulator.option(), ClientConfig()) as client:
        await client.send(SetCpuConfig(CpuConfig(sys_time_transition_margin=Duration.from_millis(20))))


asyncio.run(main())
