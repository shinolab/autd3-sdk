import asyncio
import threading
import time

from autd3 import Client, ClientConfig, Driver, UdpEmulator
from autd3.geometry import Autd3, Geometry


def drive(driver: Driver) -> None:
    while driver.poll() is not None:
        time.sleep(0.001)
    driver.close()


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    emulator = UdpEmulator(geometry.num_devices())

    driver, connector = Driver.open(emulator.option(), geometry.num_devices())
    thread = threading.Thread(target=drive, args=(driver,), daemon=True)
    thread.start()

    client = await Client.open(geometry, connector, ClientConfig())
    await client.close()

    thread.join()


asyncio.run(main())
