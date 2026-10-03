import asyncio
import collections
import math
import threading

import numpy as np

import autd3_pattern as pattern
from autd3 import Client, ClientConfig, Driver, MAX_INFLIGHT, TransportOption
from autd3.commands import ActivatePatternBank, ConfigPattern, SetSilencer, WritePatternBuffer
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3.value import Intensity, LoopBehavior, PatternBank, SamplingConfig, TransitionMode

NUM_POINTS = 1000
RADIUS_MM = 30.0


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    driver, connector = Driver.open(TransportOption(), geometry.num_devices())
    threading.Thread(target=driver.run, daemon=True).start()
    async with await Client.open(
        geometry,
        connector,
        ClientConfig(),
    ) as client:
        phases = geometry.phase_buffer()

        # ANCHOR: configure
        builder = client.datagram_builder()
        builder.push(SetSilencer.disable())
        builder.push(
            WritePatternBuffer(
                bank=PatternBank.B0,
                index=0,
                phases=phases,
                intensities=Intensity.MIN,
            )
        )
        builder.push(
            ConfigPattern(
                bank=PatternBank.B0,
                config=SamplingConfig.FREQ_40K,
                size=1,
                loop_behavior=LoopBehavior.Infinite,
            )
        )
        builder.push(
            ActivatePatternBank(
                bank=PatternBank.B0,
                transition_mode=TransitionMode.Immediate,
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
                phases,
            )
            builder = client.datagram_builder()
            builder.push(
                WritePatternBuffer(
                    bank=PatternBank.B0,
                    index=0,
                    phases=phases,
                    intensities=Intensity.MAX,
                )
            )
            for frame in builder.build():
                if len(pending) >= MAX_INFLIGHT:
                    (await pending.popleft()).check()
                pending.append(await client.send(frame))
        while pending:
            (await pending.popleft()).check()
        # ANCHOR_END: hot_loop


if __name__ == "__main__":
    asyncio.run(main())
