import asyncio

from autd3 import Client, ClientConfig, Duration, TransportOption
from autd3.commands import Clear, Synchronize
from autd3.geometry import Autd3, Geometry
from autd3.params import DEVICE_QUEUE_FRAMES


async def main() -> None:
    geometry = Geometry([Autd3()])

    udp = TransportOption()
    option = (
        # ANCHOR: config
        ClientConfig(
            ack_timeout=Duration.from_millis(10),
            max_inflight=DEVICE_QUEUE_FRAMES,
        )
        # ANCHOR_END: config
    )
    # ANCHOR: api
    client = await Client.open(geometry, udp, option)
    await client.send((Clear(), Synchronize()))
    # ANCHOR_END: api


asyncio.run(main())
