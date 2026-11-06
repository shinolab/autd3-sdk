import asyncio

from autd3 import Client, TransportOption
from autd3.commands import Clear, ReleaseFailsafe, Synchronize
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3()])
    with Client.open(geometry, TransportOption()) as client:
        await client.send(Clear())
        await client.send(Synchronize())

        msg_id = client.send_heartbeat()
        status = (await client.collect(msg_id)).device_status()
        if any(s.failsafe_active for s in status):
            await client.send(ReleaseFailsafe())


asyncio.run(main())
