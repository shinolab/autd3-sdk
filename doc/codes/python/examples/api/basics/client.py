import asyncio
import threading

from autd3 import Client, ClientConfig, Driver, UdpEmulator
from autd3.commands import Clear
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    emulator = UdpEmulator(geometry.num_devices())
    driver, connector = Driver.open(emulator.option(), geometry.num_devices())
    threading.Thread(target=driver.run, daemon=True).start()
    client = await Client.open(geometry, connector, ClientConfig())

    builder = client.datagram_builder()
    builder.push(Clear())
    frame = next(iter(builder.build()))

    # ANCHOR: api
    num_devices = client.num_devices()
    geometry = client.geometry()

    firmware = await client.read_firmware_version()
    fpga_state = await client.read_fpga_state()
    error_detail = await client.read_error_detail()

    datagram_builder = client.datagram_builder()
    resp = await (await client.send(frame))
    await client.send_checked(frame)

    await client.stop()
    await client.close()
    # ANCHOR_END: api

    _ = (num_devices, geometry, firmware, fpga_state, error_detail, datagram_builder, resp)

    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    # ANCHOR: context_manager
    emulator = UdpEmulator(geometry.num_devices())
    driver, connector = Driver.open(emulator.option(), geometry.num_devices())
    threading.Thread(target=driver.run, daemon=True).start()
    async with await Client.open(geometry, connector, ClientConfig()) as client:
        await client.send_checked(frame)
    # ANCHOR_END: context_manager


asyncio.run(main())
