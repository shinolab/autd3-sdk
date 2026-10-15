import asyncio

from autd3 import Client, ClientConfig, Duration, ServerLink
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    port = 44338
    timeout = Duration.from_secs(1)
    # ANCHOR: api
    link = ServerLink(port=port, timeout=timeout)
    client = await Client.open(geometry, link, ClientConfig())
    # ANCHOR_END: api


asyncio.run(main())
