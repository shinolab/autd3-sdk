"""
Watch the EtherCAT link status for every device.

Run with: cargo xtask py example status_check
"""

import asyncio
import signal
import threading

import autd3

CHECK_INTERVAL = 0.1


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    driver, connector = autd3.Driver.open(autd3.TransportOption(), geometry.num_devices())
    checker = driver.state_checker()
    threading.Thread(target=driver.run, daemon=True).start()
    client = await autd3.Client.open(geometry, connector, autd3.ClientConfig())

    async with client:
        print("watching link status — press Ctrl+C to stop")
        stop = asyncio.Event()
        loop = asyncio.get_running_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(sig, stop.set)

        last = None
        while not stop.is_set():
            status = checker.check()
            key = tuple(status.device_states)
            if key != last:
                for i, state in enumerate(status.device_states):
                    print(f"device[{i}]: {state}")
                print(
                    f"all ready: {status.all_ready}, any lost: {status.any_lost}"
                )
                last = key
            try:
                await asyncio.wait_for(stop.wait(), timeout=CHECK_INTERVAL)
            except asyncio.TimeoutError:
                pass


if __name__ == "__main__":
    asyncio.run(main())
