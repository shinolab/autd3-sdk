"""
One-shot (stop-and-wait) command latency in low-latency mode.

Run with: cargo xtask py example low_latency
"""

import asyncio
import time

import numpy as np

import autd3
import autd3_link_echocat as echocat
import autd3_pattern as pattern
from autd3.units import m, s

ITERATIONS = 1000
WARMUP = 10
ENABLE_LOW_LATENCY = True


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    async with await autd3.Client.open(
        geometry,
        echocat.EchocatLinkOption(),
        autd3.ClientConfig(low_latency=ENABLE_LOW_LATENCY),
    ) as client:
        print("devices:", client.num_devices())

        target = geometry.center() + np.array([0.0, 0.0, 150.0])
        wavelength = pattern.wavelength(340 * m / s)
        phases = geometry.phase_buffer()
        intensities = geometry.intensity_buffer()
        pattern.focus(geometry, target, wavelength, phases)
        pattern.set_intensity(0, intensities)
        builder = client.datagram_builder()
        builder.push(autd3.commands.Pattern(phases, intensities))
        datagrams = builder.build()

        frame = datagrams[0]
        for _ in range(WARMUP):
            await client.send_checked(frame)

        latencies = []
        for _ in range(ITERATIONS):
            t = time.perf_counter()
            await client.send_checked(frame)
            latencies.append(time.perf_counter() - t)

        latencies.sort()

        def us(seconds: float) -> float:
            return seconds * 1e6

        avg = us(sum(latencies) / ITERATIONS)
        print(f"one-shot latency over {ITERATIONS} sends (low_latency={ENABLE_LOW_LATENCY}):")
        print(
            f"  min={us(latencies[0]):.1f}us"
            f"  p50={us(latencies[ITERATIONS // 2]):.1f}us"
            f"  avg={avg:.1f}us"
            f"  p99={us(latencies[ITERATIONS * 99 // 100]):.1f}us"
            f"  max={us(latencies[-1]):.1f}us"
        )


if __name__ == "__main__":
    asyncio.run(main())
