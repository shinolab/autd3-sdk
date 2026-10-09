import asyncio
import signal

import autd3
from autd3.geometry import Autd3, Geometry
from autd3.value import Telemetry

POLL_INTERVAL = 1.0


async def main() -> None:
    _log_guard = autd3.init_tracing()

    geometry = Geometry([Autd3([0.0, 0.0, 0.0])])

    async with await autd3.Client.open(
        geometry,
        autd3.TransportOption(),
        autd3.ClientConfig(),
    ) as client:
        print("devices:", client.num_devices())
        stop = asyncio.Event()
        loop = asyncio.get_running_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(sig, stop.set)

        baseline: list[autd3.TelemetryCounters] | None = None
        print("polling telemetry — press Ctrl+C to stop")
        while not stop.is_set():
            snapshot = await client.read_telemetry()
            if baseline is None:
                print_snapshot(snapshot)
            else:
                print_deltas(baseline, snapshot)
            baseline = snapshot
            try:
                await asyncio.wait_for(stop.wait(), timeout=POLL_INTERVAL)
            except asyncio.TimeoutError:
                pass


def print_snapshot(snapshot: list[autd3.TelemetryCounters]) -> None:
    for counter in Telemetry.ALL:
        values = [counters.get(counter) for counters in snapshot]
        print(f"{counter}: {values}")


def print_deltas(before: list[autd3.TelemetryCounters], after: list[autd3.TelemetryCounters]) -> None:
    for counter in Telemetry.ALL:
        deltas = [(a.get(counter) - b.get(counter)) & 0xFFFFFFFF for b, a in zip(before, after)]
        if any(delta != 0 for delta in deltas):
            print(f"{counter}: +{deltas}")


if __name__ == "__main__":
    asyncio.run(main())
