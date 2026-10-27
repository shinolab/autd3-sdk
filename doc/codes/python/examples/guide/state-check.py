import asyncio

from autd3 import Client, DeviceState, DeviceStatus, TransportOption
from autd3.commands import ReleaseFailsafe, each
from autd3.geometry import Autd3, Geometry

# xtask:long-running  # [hide]

CHECK_INTERVAL = 0.1


async def main() -> None:
    geometry = Geometry([Autd3()])

    # ANCHOR: open
    client = Client.open(geometry, TransportOption())
    # ANCHOR_END: open

    with client:
        # ANCHOR: ptp_failsafe
        client.send_heartbeat()
        await asyncio.sleep(CHECK_INTERVAL)
        status = client.device_status()

        def releasable(s: DeviceStatus) -> bool:
            return s.failsafe_active and s.ptp_locked

        if any(releasable(s) for s in status):
            await client.send(each(lambda device: ReleaseFailsafe() if releasable(status[device.idx()]) else None))
        # ANCHOR_END: ptp_failsafe

        # ANCHOR: poll
        last = None
        while True:
            client.send_heartbeat()
            status = client.device_status()
            if status != last:
                for i, s in enumerate(status):
                    print(f"device[{i}]: {s.state}")
                all_ready = all(s.state == DeviceState.Ready for s in status)
                any_lost = any(s.state == DeviceState.Lost for s in status)
                print(f"all ready: {all_ready}, any lost: {any_lost}")
                last = status
            await asyncio.sleep(CHECK_INTERVAL)
        # ANCHOR_END: poll


if __name__ == "__main__":
    asyncio.run(main())
