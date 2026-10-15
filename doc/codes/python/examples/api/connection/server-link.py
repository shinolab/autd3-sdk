import asyncio

from autd3 import Client, ClientConfig, Duration, ServerLink
from autd3.geometry import Autd3, Geometry
from autd3.params import SERVER_PORT


async def main() -> None:
    geometry = Geometry([Autd3()])

    port = SERVER_PORT
    timeout = Duration.from_secs(1)
    # ANCHOR: api
    link = ServerLink(port=port, timeout=timeout)
    client = await Client.open(geometry, link, ClientConfig())
    # ANCHOR_END: api


asyncio.run(main())
