"""Two simultaneous foci synthesized with GS-PAT. Run with: cargo xtask py example holo"""

import asyncio
import signal

import numpy as np

import autd3
import autd3_link_ethercrab as ethercrab
import autd3_modulation as modulation
import autd3_pattern as pattern
import autd3_pattern_holo as holo


async def main() -> None:
    geometry = autd3.Geometry([autd3.Autd3()])

    client = await autd3.Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        autd3.ClientConfig(),
    )

    center = geometry.center()
    wavelength = pattern.wavelength(340_000.0)
    foci = [
        holo.ControlPoint(center + np.array([-20.0, 0.0, 150.0]), holo.Amplitude.spl(150.0)),
        holo.ControlPoint(center + np.array([20.0, 0.0, 150.0]), holo.Amplitude.spl(150.0)),
    ]

    patterns = geometry.pattern_buffer()
    holo.gspat(geometry, foci, wavelength, holo.GspatOption(repeat=100), patterns)

    mod_buf = modulation.modulation_buffer()
    modulation.sine(200.0, modulation.SineOption(), mod_buf)

    builder = client.datagram_builder()
    builder.push(autd3.Pattern(patterns))
    builder.push(autd3.Modulation(autd3.SamplingConfig.FREQ_4K, mod_buf))
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
