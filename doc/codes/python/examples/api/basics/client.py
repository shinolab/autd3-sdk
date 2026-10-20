import asyncio

from autd3 import Client, Duration, ExecAt, Frames, TransportOption
from autd3.commands import Clear, Nop, Synchronize
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3()])
    # ANCHOR: open
    client = Client.open(geometry, TransportOption())
    await client.send((Clear(), Synchronize()))
    # ANCHOR_END: open

    # ANCHOR: exec_at
    frames = Frames.encode(client.geometry(), Nop())

    lead = ExecAt.Lead(Duration.from_millis(1))
    for frame in frames:
        (await (await client.send_frame_at(frame, lead))).check()

    at = client.device_time_now() + Duration.from_millis(5)
    for frame in frames:
        (await (await client.send_frame_at(frame, ExecAt.At(at)))).check()
    # ANCHOR_END: exec_at
    frame = next(iter(frames))

    # ANCHOR: heartbeat
    for _ in range(100):
        client.send_heartbeat()
        await asyncio.sleep(0.01)
    # ANCHOR_END: heartbeat

    # ANCHOR: api
    num_devices = client.num_devices()
    geometry = client.geometry()

    client.send_heartbeat()

    firmware = await client.read_firmware_version()
    fpga_state = await client.read_fpga_state()

    await client.send(Nop())
    resp = await (await client.send_frame(frame))

    await client.close()
    # ANCHOR_END: api

    _ = (num_devices, geometry, firmware, fpga_state, resp)

    geometry = Geometry([Autd3()])

    # ANCHOR: context_manager
    async with Client.open(geometry, TransportOption()) as client:
        await client.send(Nop())
    # ANCHOR_END: context_manager


asyncio.run(main())
