import asyncio

from autd3 import Client, ClientConfig, Duration, ExecAt, Frames, TransportOption
from autd3.commands import Nop
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3()])
    client = await Client.open(geometry, TransportOption(), ClientConfig())

    frame = next(iter(Frames.encode(geometry, Nop())))

    # ANCHOR: exec_at
    at = client.device_time_now() + Duration.from_millis(5)
    (await (await client.send_frame_at(frame, ExecAt.At(at)))).check()

    lead = ExecAt.Lead(Duration.from_millis(1))
    (await (await client.send_frame_at(frame, lead))).check()

    (await (await client.send_frame_at(frame, ExecAt.Immediate))).check()
    # ANCHOR_END: exec_at

    # ANCHOR: heartbeat
    for _ in range(100):
        client.heartbeat()
        await asyncio.sleep(0.01)
    # ANCHOR_END: heartbeat

    # ANCHOR: api
    num_devices = client.num_devices()
    geometry = client.geometry()

    client.heartbeat()

    config = client.config()
    client.set_config(ClientConfig(ack_timeout=Duration.from_millis(20)))

    firmware = await client.read_firmware_version()
    fpga_state = await client.read_fpga_state()

    await client.send(Nop())
    done = await client.send_streaming(Nop())
    resp = await (await client.send_frame(frame))

    await client.close()
    # ANCHOR_END: api

    _ = (num_devices, geometry, config, firmware, fpga_state, done, resp)

    geometry = Geometry([Autd3()])

    # ANCHOR: context_manager
    async with await Client.open(geometry, TransportOption(), ClientConfig()) as client:
        await client.send(Nop())
    # ANCHOR_END: context_manager


asyncio.run(main())
