import asyncio
import threading

from autd3 import Client, ClientConfig, Driver, UdpEmulator
from autd3.commands import SetPhaseCorrection
from autd3.geometry import Autd3, Geometry
from autd3.value import Phase


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    emulator = UdpEmulator(geometry.num_devices())
    driver, connector = Driver.open(emulator.option(), geometry.num_devices())
    threading.Thread(target=driver.run, daemon=True).start()
    async with await Client.open(geometry, connector, ClientConfig()) as client:
        phases = [
            [Phase.ZERO] * geometry.device(i).num_transducers()
            for i in range(geometry.num_devices())
        ]

        builder = client.datagram_builder()
        builder.push(SetPhaseCorrection(phases=phases))
        frames = builder.build()
        for frame in frames:
            await client.send_checked(frame)


asyncio.run(main())
