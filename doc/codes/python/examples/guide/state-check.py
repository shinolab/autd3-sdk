import asyncio

from autd3 import Client, DeviceMask, TransportOption
from autd3.commands import ReleaseFailsafe, each
from autd3.geometry import Autd3, Geometry

# xtask:long-running  # [hide]

CHECK_INTERVAL = 0.1


async def main() -> None:
    geometry = Geometry([Autd3()])

    # ANCHOR: open
    client = Client.open(geometry, TransportOption())
    checker = client.state_checker()
    # ANCHOR_END: open

    async with client:
        # ANCHOR: ptp_failsafe
        client.send_heartbeat()
        await asyncio.sleep(CHECK_INTERVAL)
        status = checker.check()
        if status.any_ptp_failsafe:
            releasable = DeviceMask.from_devices(i for i in status.ptp_failsafe if status.ptp_locked[i])
            if releasable:
                await client.send(each(lambda device: ReleaseFailsafe() if releasable[device.idx()] else None))
        # ANCHOR_END: ptp_failsafe

        # ANCHOR: poll
        last = None
        while True:
            client.send_heartbeat()
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
