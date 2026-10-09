import asyncio

from autd3 import Client, ClientConfig, Frames, UdpEmulator
from autd3.commands import Nop
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    emulator = UdpEmulator(geometry.num_devices())
    client = await Client.open(geometry, emulator.option(), ClientConfig())

    frame = next(iter(Frames.encode(geometry, Nop())))

    # ANCHOR: api
    num_devices = client.num_devices()
    geometry = client.geometry()

    firmware = await client.read_firmware_version()
    fpga_state = await client.read_fpga_state()

    await client.send(Nop())
    done = await client.send_streaming(Nop())
    resp = await (await client.send_frame(frame))

    await client.silent_stop()
    await client.close()
    # ANCHOR_END: api

    _ = (num_devices, geometry, firmware, fpga_state, done, resp)

    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    # ANCHOR: context_manager
    emulator = UdpEmulator(geometry.num_devices())
    async with await Client.open(geometry, emulator.option(), ClientConfig()) as client:
        await client.send(Nop())
    # ANCHOR_END: context_manager


asyncio.run(main())
