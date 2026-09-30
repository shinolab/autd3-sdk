import asyncio
import threading

from autd3 import Client, ClientConfig, Driver, TransportOption
from autd3.geometry import Autd3, Geometry

# xtask:long-running  # [hide]

CHECK_INTERVAL = 0.1


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    # ANCHOR: open
    driver, connector = Driver.open(TransportOption(), geometry.num_devices())
    checker = driver.state_checker()
    threading.Thread(target=driver.run, daemon=True).start()
    client = await Client.open(geometry, connector, ClientConfig())
    # ANCHOR_END: open

    async with client:
        # ANCHOR: poll
        last = None
        while True:
            status = checker.check()
            if status != last:
                for i, state in enumerate(status.device_states):
                    print(f"device[{i}]: {state}")
                print(f"all ready: {status.all_ready}, any lost: {status.any_lost}")
                last = status
            await asyncio.sleep(CHECK_INTERVAL)
        # ANCHOR_END: poll


if __name__ == "__main__":
    asyncio.run(main())
