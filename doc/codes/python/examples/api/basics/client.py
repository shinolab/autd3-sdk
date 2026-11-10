import asyncio

from autd3 import Client, TransportOption
from autd3.commands import Clear, Nop, Synchronize
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3()])
    # ANCHOR: open
    client = await Client.open(geometry, TransportOption())
    await client.send(Clear())
    await client.send(Synchronize())
    # ANCHOR_END: open

    # ANCHOR: api
    num_devices = client.num_devices()
    now = client.device_time_now()

    firmware = await client.read_firmware_version()

    await client.send(Nop())
    await client.send_streaming(Nop())

    state = await asyncio.wait_for(client.read_state(), timeout=0.2)
    fpga_state = [r.fpga_state for r in state.replies]
    # ANCHOR_END: api

    _ = (num_devices, now, firmware, fpga_state)

    geometry = Geometry([Autd3()])

    # ANCHOR: context_manager
    with await Client.open(geometry, TransportOption()) as client:
        await client.send(Nop())
    # ANCHOR_END: context_manager


asyncio.run(main())
