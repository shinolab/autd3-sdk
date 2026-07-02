import asyncio

import autd3
import autd3_link_nop as nop


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    client = await autd3.Client.open(geometry, nop.Nop(), autd3.ClientConfig())

    builder = client.datagram_builder()
    builder.push(autd3.commands.Clear())
    frame = next(iter(builder.build()))

    # ANCHOR: api
    num_devices = client.num_devices()

    firmware = await client.read_firmware_version()
    fpga_state = await client.read_fpga_state()
    error_detail = await client.read_error_detail()

    datagram_builder = client.datagram_builder()
    resp = await (await client.send(frame))
    await client.send_checked(frame)

    await client.stop()
    await client.close()
    # ANCHOR_END: api

    _ = (num_devices, firmware, fpga_state, error_detail, datagram_builder, resp)


asyncio.run(main())
