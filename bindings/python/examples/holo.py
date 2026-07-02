"""Two simultaneous foci synthesized with GS-PAT. Run with: cargo xtask py example holo"""

import asyncio
import signal

import numpy as np

import autd3
import autd3_link_ethercrab as ethercrab
import autd3_modulation as modulation
import autd3_pattern as pattern
import autd3_pattern_holo as holo
from autd3.units import Hz, m, s
from autd3_pattern_holo import dB


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    client = await autd3.Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        autd3.ClientConfig(),
    )

    center = geometry.center()
    wavelength = pattern.wavelength(340 * m / s)
    foci = [
        holo.ControlPoint(center + np.array([-20.0, 0.0, 150.0]), 150 * dB),
        holo.ControlPoint(center + np.array([20.0, 0.0, 150.0]), 150 * dB),
    ]

    patterns = geometry.pattern_buffer()
    holo.gspat(geometry, foci, wavelength, holo.GspatOption(repeat=100), patterns)

    mod_buf = modulation.modulation_buffer()
    modulation.sine(200 * Hz, modulation.SineOption(), mod_buf)

    builder = client.datagram_builder()
    builder.push(autd3.commands.Pattern(patterns))
    builder.push(autd3.commands.Modulation(autd3.value.SamplingConfig.FREQ_4K, mod_buf))
    for frame in builder.build():
        await client.send_checked(frame)

    print("emitting two GS-PAT foci with a 200 Hz AM — press Ctrl+C to stop")
    stop = asyncio.Event()
    loop = asyncio.get_running_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, stop.set)
    await stop.wait()

    await client.stop()
    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
