import asyncio
import collections
import math

import numpy as np

import autd3_pattern as pattern
from autd3 import Client, ClientConfig, Frames, MAX_INFLIGHT, TransportOption
from autd3.commands import ActivatePatternBank, ConfigPattern, SetSilencer, WritePatternBuffer
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3.value import Intensity, LoopBehavior, PatternBank, SamplingConfig, TransitionMode

NUM_POINTS = 1000
RADIUS_MM = 30.0


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    async with await Client.open(
        geometry,
        TransportOption(),
        ClientConfig(),
    ) as client:
        phases = geometry.phase_buffer()

        # ANCHOR: configure
        await client.send(SetSilencer.disable())
        await client.send(
            WritePatternBuffer(
                bank=PatternBank.B0,
                index=0,
                phases=phases,
                intensities=Intensity.MIN,
            )
        )
        await client.send(
            ConfigPattern(
                bank=PatternBank.B0,
                config=SamplingConfig.FREQ_40K,
                size=1,
                loop_behavior=LoopBehavior.Infinite,
            )
        )
        await client.send(
            ActivatePatternBank(
                bank=PatternBank.B0,
                transition_mode=TransitionMode.Immediate,
            )
        )
        # ANCHOR_END: configure

        center = geometry.center() + np.array([0.0, 0.0, 150.0])
        wavelength = pattern.wavelength(340 * m / s)

        # ANCHOR: hot_loop
        frames = Frames()
        pending = collections.deque()
        for i in range(NUM_POINTS):
            theta = 2.0 * math.pi * i / NUM_POINTS
            target = center + np.array([RADIUS_MM * math.cos(theta), RADIUS_MM * math.sin(theta), 0.0])
            pattern.focus(
                geometry,
                target,
                wavelength,
                phases,
            )
            frames.encode_into(
                geometry,
                WritePatternBuffer(
                    bank=PatternBank.B0,
                    index=0,
                    phases=phases,
                    intensities=Intensity.MAX,
                ),
            )
            for frame in frames:
                if len(pending) >= MAX_INFLIGHT:
                    (await pending.popleft()).check()
                pending.append(await client.send_frame(frame))
        while pending:
            (await pending.popleft()).check()
        # ANCHOR_END: hot_loop


if __name__ == "__main__":
    asyncio.run(main())
