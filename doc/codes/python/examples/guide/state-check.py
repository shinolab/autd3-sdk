import asyncio

from autd3 import Client, DeviceState, TransportOption
from autd3.geometry import Autd3, Geometry

# xtask:long-running  # [hide]

CHECK_INTERVAL = 0.1


async def main() -> None:
    geometry = Geometry([Autd3()])

    # ANCHOR: open
    client = await Client.open(geometry, TransportOption())
    # ANCHOR_END: open

    with client:
        # ANCHOR: poll
        last = None
        while True:
            try:
                response = await asyncio.wait_for(client.read_state(), timeout=CHECK_INTERVAL)
            except TimeoutError:
                print(f"not every device replied within {CHECK_INTERVAL} s")
                continue
            status = response.device_status()
            if status != last:
                for i, s in enumerate(status):
                    print(f"device[{i}]: {s.state}")
                all_ready = all(s.state == DeviceState.Ready for s in status)
                print(f"all ready: {all_ready}")
                last = status
            await asyncio.sleep(CHECK_INTERVAL)
        # ANCHOR_END: poll


if __name__ == "__main__":
    asyncio.run(main())
