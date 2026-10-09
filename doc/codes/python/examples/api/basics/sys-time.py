import asyncio

from autd3 import Client, ClientConfig, Duration, UdpEmulator
from autd3.geometry import Autd3, Geometry
from autd3.value import SysTime


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    emulator = UdpEmulator(geometry.num_devices())
    async with await Client.open(geometry, emulator.option(), ClientConfig()) as client:
        # ANCHOR: construct
        opened = SysTime.ZERO
        now = client.device_time_now()
        raw = SysTime.from_nanos(1_000_000_000)
        ns = now.sys_time
        # ANCHOR_END: construct

        # ANCHOR: ops
        future = client.device_time_now() + Duration.from_millis(100)
        past = future - Duration.from_millis(50)
        elapsed = future - past
        is_after = future > past
        # ANCHOR_END: ops

        _ = (opened, raw, ns, past, elapsed, is_after)


asyncio.run(main())
