import asyncio
import collections
import math

import numpy as np

import autd3
import autd3_link_ethercrab as ethercrab
import autd3_pattern as pattern
from autd3.units import m, s

NUM_POINTS = 1000
RADIUS_MM = 30.0


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    client = await autd3.Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        autd3.ClientConfig(),
    )

    patterns = geometry.pattern_buffer()

    # ANCHOR: configure
    builder = client.datagram_builder()
    builder.push(autd3.commands.SetSilencer.disable())
    builder.push(
        autd3.commands.WritePatternBuffer(
            autd3.value.PatternBank.B0,
            0,
            patterns,
        )
    )
    builder.push(
        autd3.commands.ConfigPattern(
            autd3.value.PatternBank.B0,
            autd3.value.SamplingConfig.FREQ_40K,
            1,
            loop_behavior=autd3.value.LoopBehavior.Infinite,
        )
    )
    for frame in builder.build():
        await client.send_checked(frame)
    # ANCHOR_END: configure

    center = geometry.center() + np.array([0.0, 0.0, 150.0])
    wavelength = pattern.wavelength(340 * m / s)

    # ANCHOR: hot_loop
    pending = collections.deque()
    for i in range(NUM_POINTS):
        theta = 2.0 * math.pi * i / NUM_POINTS
        target = center + np.array([RADIUS_MM * math.cos(theta), RADIUS_MM * math.sin(theta), 0.0])
        pattern.focus(
            geometry,
            target,
            wavelength,
            pattern.FocusOption(),
            patterns,
        )
        builder = client.datagram_builder()
        builder.push(
            autd3.commands.WritePatternBuffer(
                autd3.value.PatternBank.B0,
                0,
                patterns,
            )
        )
        for frame in builder.build():
            if len(pending) >= autd3.MAX_IN_FLIGHT:
                await pending.popleft()
            pending.append(await client.send(frame))
    while pending:
        await pending.popleft()
    # ANCHOR_END: hot_loop

    await client.stop()
    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
