"""One-shot (stop-and-wait) command latency in low-latency mode.
Run with: cargo xtask py example low_latency"""

import asyncio
import time

import numpy as np

import autd3
import autd3_link_ethercrab as ethercrab
import autd3_pattern as pattern

ITERATIONS = 1000
WARMUP = 10
ENABLE_LOW_LATENCY = True


async def main() -> None:
    geometry = autd3.Geometry([autd3.Autd3()])

    client = await autd3.Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        autd3.ClientConfig(low_latency=ENABLE_LOW_LATENCY),
    )

    print("devices:", client.num_devices())

    target = geometry.center() + np.array([0.0, 0.0, 150.0])
    wavelength = pattern.wavelength(340_000.0)
    patterns = client.pattern_buffer()
    pattern.focus(geometry, target, wavelength, autd3.Intensity.MIN, patterns)
    builder = client.datagram_builder()
    builder.push(autd3.Pattern(patterns))
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

    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
