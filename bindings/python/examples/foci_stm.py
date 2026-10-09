"""
Sweeps a focus around a 30 mm circle at 1 Hz using FociStm.

Run with: cargo xtask py example foci_stm
"""

import asyncio
import signal

import autd3
from autd3.commands import FociStm, FociStmOption, SetSilencer, circle
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, mm
from autd3.value import ControlPoints, Intensity


async def main() -> None:
    _log_guard = autd3.init_tracing()

    geometry = Geometry([Autd3([0.0, 0.0, 0.0])])

    async with await autd3.Client.open(
        geometry,
        autd3.TransportOption(),
        autd3.ClientConfig(),
    ) as client:
        print("devices:", client.num_devices())

        center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
        points: list[ControlPoints] = []
        circle(center, 30.0 * mm, 200, [0.0, 0.0, 1.0], Intensity.MAX, points)

        await client.send(SetSilencer())
        await (await client.send_streaming(FociStm(1.0 * Hz, points, FociStmOption())))

        print("running a 1 Hz circular foci STM — press Ctrl+C to stop")
        stop = asyncio.Event()
        loop = asyncio.get_running_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(sig, stop.set)
        await stop.wait()

        await client.silent_stop()


if __name__ == "__main__":
    asyncio.run(main())
