import asyncio
import threading

from autd3 import Client, ClientConfig, Driver, UdpEmulator
from autd3.commands import SetSilencer
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry(
        [
            Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
            Autd3([192.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
        ]
    )
    emulator = UdpEmulator(geometry.num_devices())
    driver, connector = Driver.open(emulator.option(), geometry.num_devices())
    threading.Thread(target=driver.run, daemon=True).start()
    async with await Client.open(geometry, connector, ClientConfig()) as client:
        builder = client.datagram_builder()
        builder.push(SetSilencer())
        for frame in builder.build():
            await client.send_checked(frame)

    emulator.reboot(1)


asyncio.run(main())
