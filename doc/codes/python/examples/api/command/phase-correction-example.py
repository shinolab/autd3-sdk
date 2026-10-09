import asyncio

from autd3 import Client, ClientConfig, UdpEmulator
from autd3.commands import SetPhaseCorrection
from autd3.geometry import Autd3, Geometry
from autd3.value import Phase


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    emulator = UdpEmulator(geometry.num_devices())
    async with await Client.open(geometry, emulator.option(), ClientConfig()) as client:
        phases = [
            [Phase.ZERO] * geometry.device(i).num_transducers()
            for i in range(geometry.num_devices())
        ]

        await client.send(SetPhaseCorrection(phases=phases))


asyncio.run(main())
