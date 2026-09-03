import asyncio

import autd3_link_nop as nop
from autd3 import Client, ClientConfig
from autd3.commands import Clear
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    client = await Client.open(geometry, nop.Nop(), ClientConfig())

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
    async with await Client.open(geometry, nop.Nop(), ClientConfig()) as client:
        await client.send_checked(frame)
    # ANCHOR_END: context_manager


asyncio.run(main())
