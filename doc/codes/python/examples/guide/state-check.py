import asyncio

from autd3 import Client, DeviceState, TransportOption
from autd3.geometry import Autd3, Geometry

# xtask:long-running  # [hide]

CHECK_INTERVAL = 0.1


async def main() -> None:
    geometry = Geometry([Autd3()])

    # ANCHOR: open
    client = Client.open(geometry, TransportOption())
    # ANCHOR_END: open

    with client:
        # ANCHOR: poll
        last = None
        while True:
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
