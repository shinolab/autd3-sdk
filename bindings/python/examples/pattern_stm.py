"""
Pattern STM: a circle of host-computed focus patterns played back at 1 Hz.

Run with: cargo xtask py example pattern_stm
"""

import asyncio
import math
import signal

import autd3
import autd3_pattern as pattern
from autd3.commands import PatternStm, PatternStmOption, SetSilencer
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, m, mm, s
from autd3.value import Intensity

NUM_POINTS = 200
RADIUS_MM = 30.0


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
        wavelength = pattern.wavelength(340 * m / s)
        patterns = []
        for i in range(NUM_POINTS):
            theta = 2.0 * math.pi * i / NUM_POINTS
            target = center + offset(
                RADIUS_MM * math.cos(theta) * mm,
                RADIUS_MM * math.sin(theta) * mm,
                0.0 * mm,
            )
            phases = geometry.phase_buffer()
            pattern.focus(geometry, target, wavelength, phases)
            patterns.append(phases)

        await client.send(SetSilencer())
        await (await client.send_streaming(PatternStm(1.0 * Hz, patterns, Intensity.MAX, PatternStmOption())))

        print("running a 1 Hz circular pattern STM — press Ctrl+C to stop")
        stop = asyncio.Event()
        loop = asyncio.get_running_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(sig, stop.set)
        await stop.wait()

        await client.silent_stop()


if __name__ == "__main__":
    asyncio.run(main())
