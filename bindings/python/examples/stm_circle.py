"""Sweeps a focus around a 30 mm circle at 1 Hz using FociStm.

Run with: cargo xtask py example stm_circle
"""

import asyncio
import signal

import numpy as np

import autd3
import autd3_link_ethercrab as ethercrab
from autd3.units import Hz


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    client = await autd3.Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        autd3.ClientConfig(),
    )

    center = geometry.center() + np.array([0.0, 0.0, 150.0])
    samples = []
    autd3.commands.circle(center, 30.0, 200, [0.0, 0.0, 1.0], autd3.value.Intensity.MAX, samples)
    stm = autd3.commands.FociStm(1.0 * Hz, samples, autd3.commands.FociStmOption())

    builder = client.datagram_builder()
    builder.push(stm)
    for frame in builder.build():
        await client.send_checked(frame)

    print("sweeping a focus around a 30 mm circle at 1 Hz — press Ctrl+C to stop")
    stop = asyncio.Event()
    loop = asyncio.get_running_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, stop.set)
    await stop.wait()

    await client.stop()
    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
