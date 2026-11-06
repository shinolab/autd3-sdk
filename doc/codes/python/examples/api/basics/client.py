import asyncio

from autd3 import Client, Duration, ExecAt, Frames, TransportOption
from autd3.commands import Clear, Nop, Synchronize
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3()])
    # ANCHOR: open
    client = Client.open(geometry, TransportOption())
    await client.send(Clear())
    await client.send(Synchronize())
    # ANCHOR_END: open

    # ANCHOR: exec_at
    frames = Frames.encode(client.geometry(), Nop())

    lead = ExecAt.Lead(Duration.from_millis(1))
    for frame in frames:
        msg_id = await client.send_frame(frame, lead)
        (await client.collect(msg_id)).check()

    at = client.device_time_now() + Duration.from_millis(5)
    for frame in frames:
        msg_id = await client.send_frame(frame, ExecAt.At(at))
        (await client.collect(msg_id)).check()
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

    msg_id = client.send_heartbeat()
    heartbeat = await client.collect(msg_id)
    client.send_heartbeat()
    reply = await client.recv()

    firmware = await client.read_firmware_version()
    fpga_state = [r.fpga_state for r in heartbeat.replies]

    await client.send(Nop())
    msg_id = await client.send_frame(frame, ExecAt.Immediate)
    resp = await asyncio.wait_for(client.collect(msg_id), timeout=0.2)
    # ANCHOR_END: api

    _ = (num_devices, geometry, heartbeat, reply, firmware, fpga_state, resp)

    geometry = Geometry([Autd3()])

    # ANCHOR: context_manager
    with Client.open(geometry, TransportOption()) as client:
        await client.send(Nop())
    # ANCHOR_END: context_manager


asyncio.run(main())
