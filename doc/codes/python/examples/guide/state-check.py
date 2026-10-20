import asyncio

from autd3 import Client, TransportOption
from autd3.commands import ReleaseFailsafe, Synchronize, each
from autd3.geometry import Autd3, Geometry

# xtask:long-running  # [hide]

CHECK_INTERVAL = 0.1


async def main() -> None:
    geometry = Geometry([Autd3()])

    # ANCHOR: open
    client = await Client.open(geometry, TransportOption())
    checker = client.state_checker()
    # ANCHOR_END: open

    async with client:
        # ANCHOR: recover
        if checker.check().any_lost:
            recovered = await client.recover()
            if recovered:
                await client.send(each(lambda device: Synchronize() if recovered[device.idx()] else None))
                await client.send(ReleaseFailsafe())
        # ANCHOR_END: recover

        # ANCHOR: poll
        last = None
        while True:
            client.heartbeat()
            status = checker.check()
            if status != last:
                for i, state in enumerate(status.devices):
                    print(f"device[{i}]: {state}")
                print(f"all ready: {status.all_ready}, any lost: {status.any_lost}")
                last = status
            await asyncio.sleep(CHECK_INTERVAL)
        # ANCHOR_END: poll


if __name__ == "__main__":
    asyncio.run(main())
