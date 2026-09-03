"""
Watch the EtherCAT link status for every device.

Run with: cargo xtask py example status_check
"""

import asyncio
import signal

import autd3
import autd3_link_echocat as echocat

CHECK_INTERVAL = 0.1


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    client, checker = await autd3.Client.open_with_checker(
        geometry,
        echocat.EchocatLinkOption(),
        autd3.ClientConfig(),
    )

    async with client:
        print("watching link status — press Ctrl+C to stop")
        stop = asyncio.Event()
        loop = asyncio.get_running_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(sig, stop.set)

        last = None
        while not stop.is_set():
            status = checker.check()
            key = (tuple(status.device_states), status.recoveries)
            if key != last:
                for i, state in enumerate(status.device_states):
                    print(f"device[{i}]: {state}")
                print(
                    f"all operational: {status.all_op}, "
                    f"any lost: {status.any_lost}, recoveries: {status.recoveries}"
                )
                last = key
            try:
                await asyncio.wait_for(stop.wait(), timeout=CHECK_INTERVAL)
            except asyncio.TimeoutError:
                pass


if __name__ == "__main__":
    asyncio.run(main())
