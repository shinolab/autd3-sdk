"""
Pattern STM: a circle of host-computed focus patterns played back at 1 Hz.

Run with: cargo xtask py example pattern_stm
"""

import asyncio
import math
import signal

import numpy as np

import autd3
import autd3_link_echocat as echocat
import autd3_pattern as pattern
from autd3.units import Hz, m, s

NUM_POINTS = 200
RADIUS_MM = 30.0


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    client = await autd3.Client.open(
        geometry,
        echocat.EchocatLinkOption(),
        autd3.ClientConfig(),
    )

    print("devices:", client.num_devices())

    center = geometry.center() + np.array([0.0, 0.0, 150.0])
    wavelength = pattern.wavelength(340 * m / s)
    focus_option = pattern.FocusOption()
    patterns = []
    for i in range(NUM_POINTS):
        theta = 2.0 * math.pi * i / NUM_POINTS
        target = center + np.array([RADIUS_MM * math.cos(theta), RADIUS_MM * math.sin(theta), 0.0])
        buffer = geometry.pattern_buffer()
        pattern.focus(geometry, target, wavelength, focus_option, buffer)
        patterns.append(buffer)

    builder = client.datagram_builder()
    builder.push(autd3.commands.SetSilencer())
    builder.push(
        autd3.commands.PatternStm(
            1.0 * Hz,
            patterns,
            autd3.commands.PatternStmOption(mode=autd3.commands.PatternStmMode.PhaseFull),
        )
    )
    for frame in builder.build():
        await client.send_checked(frame)

    print("running a 1 Hz circular pattern STM — press Ctrl+C to stop")
    stop = asyncio.Event()
    loop = asyncio.get_running_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, stop.set)
    await stop.wait()

    await client.stop()
    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
