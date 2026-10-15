import asyncio

from autd3 import Client, ClientConfig, Duration, UdpOption
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    udp = UdpOption()
    option = (
        # ANCHOR: config
        ClientConfig(
            ack_timeout=Duration.from_millis(10),
            max_inflight=7,
            max_resync_rounds=8,
            require_supported_firmware=False,
            resynchronize_on_recovery=3,
            release_failsafe_on_recovery=3,
        )
        # ANCHOR_END: config
    )
    # ANCHOR: api
    await Client.open(geometry, udp, option)
    # ANCHOR_END: api


asyncio.run(main())
