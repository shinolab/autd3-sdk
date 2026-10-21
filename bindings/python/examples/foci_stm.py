"""
Sweeps a focus around a 30 mm circle at 1 Hz using FociStm.

Run with: cargo xtask py example foci_stm
"""

import asyncio
import signal

import numpy as np

import autd3
from autd3.commands import Clear, FociStm, FociStmOption, SetSilencer, Synchronize
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, mm
from autd3.value import ControlPoint, ControlPoints, Intensity, Phase, SamplingConfig


async def main() -> None:
    _log_guard = autd3.init_tracing()

    geometry = Geometry([Autd3([0.0, 0.0, 0.0])])

    async with autd3.Client.open(
        geometry,
        autd3.TransportOption(),
    ) as client:
        await client.send((Clear(), Synchronize()))
        print("devices:", client.num_devices())

        center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
        points = [
            ControlPoints(
                [ControlPoint(center + 30.0 * np.array([np.cos(theta), np.sin(theta), 0.0]), Phase.ZERO)],
                Intensity.MAX,
            )
            for theta in np.linspace(0.0, 2.0 * np.pi, 200, endpoint=False)
        ]

        await client.send(SetSilencer())
        config = SamplingConfig(1.0 * Hz * len(points))
        await client.send(FociStm(config, points, FociStmOption()))

        print("running a 1 Hz circular foci STM — press Ctrl+C to stop")
        stop = asyncio.Event()
        loop = asyncio.get_running_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(sig, stop.set)
        await stop.wait()


if __name__ == "__main__":
    asyncio.run(main())
