import asyncio
import math

import numpy as np

from autd3 import Client, ClientConfig
from autd3.commands import PatternStm, PatternStmMode, PatternStmOption, StmConfig
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz, m, s
from autd3.value import LoopBehavior, PatternBank, TransitionMode
from autd3_link_nop import Nop
from autd3_pattern import FocusOption, focus, wavelength


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    client = await Client.open(geometry, Nop(), ClientConfig())

    center = geometry.center() + np.array([0.0, 0.0, 150.0])
    wl = wavelength(340 * m / s)
    patterns = []
    for i in range(200):
        theta = 2.0 * math.pi * i / 200
        target = center + np.array([30.0 * math.cos(theta), 30.0 * math.sin(theta), 0.0])
        buffer = geometry.pattern_buffer()
        focus(
            geometry,
            target,
            wl,
            FocusOption(),
            buffer,
        )
        patterns.append(buffer)

    builder = client.datagram_builder()
    builder.push(
        PatternStm(
            1.0 * Hz,
            patterns,
            PatternStmOption(
                bank=PatternBank.B0,
                mode=PatternStmMode.PhaseIntensityFull,
                loop_behavior=LoopBehavior.Infinite,
                transition_mode=TransitionMode.Immediate,
            ),
        )
    )
    frames = builder.build()
    for frame in frames:
        await client.send_checked(frame)

    await client.close()


asyncio.run(main())
