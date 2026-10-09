"""
Watch the link status for every device.

Run with: cargo xtask py example status_check
"""

import asyncio
import signal

import autd3
from autd3.geometry import Autd3, Geometry

CHECK_INTERVAL = 0.1


async def main() -> None:
    _log_guard = autd3.init_tracing()

    geometry = Geometry([Autd3([0.0, 0.0, 0.0])])

    client = await autd3.Client.open(geometry, autd3.TransportOption(), autd3.ClientConfig())
    checker = client.state_checker()

    async with client:
        print("watching device status — press Ctrl+C to stop")
        stop = asyncio.Event()
        loop = asyncio.get_running_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(sig, stop.set)

        last: autd3.DeviceStatus | None = None
        while not stop.is_set():
            status = checker.check()
            if status != last:
                print_status(status)
                last = status
            try:
                await asyncio.wait_for(stop.wait(), timeout=CHECK_INTERVAL)
            except asyncio.TimeoutError:
                pass


def print_status(status: autd3.DeviceStatus) -> None:
    for i, state in enumerate(status.devices):
        print(f"device[{i}]: {state}")
    print(f"all ready: {status.all_ready}, any lost: {status.any_lost}")


if __name__ == "__main__":
    asyncio.run(main())
