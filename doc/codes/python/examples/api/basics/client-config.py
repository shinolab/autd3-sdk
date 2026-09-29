import asyncio

from autd3 import Client, ClientConfig, Duration, RtPriority, RtSchedulePolicy, TransportOption
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    udp = TransportOption()
    option = (
        # ANCHOR: config
        ClientConfig(
            ack_timeout=Duration.from_millis(10),
            max_inflight=7,
            max_resync_rounds=8,
            low_latency=False,
            rt_priority=RtPriority(80),
            rt_policy=RtSchedulePolicy.Fifo,
            rt_affinity=None,
            validate_state=True,
            require_supported_firmware=False,
        )
        # ANCHOR_END: config
    )
    # ANCHOR: api
    await Client.open(geometry, udp, option)
    # ANCHOR_END: api


asyncio.run(main())
