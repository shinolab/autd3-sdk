"""
Two simultaneous foci synthesized with GS-PAT.

Run with: cargo xtask py example holo
"""

import asyncio
import signal

import autd3
import autd3_modulation as modulation
import autd3_pattern as pattern
import autd3_pattern_holo as holo
from autd3.commands import Modulation, Pattern, SetSilencer
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, m, mm, s
from autd3.value import SamplingConfig
from autd3_pattern_holo import Pa


async def main() -> None:
    _log_guard = autd3.init_tracing()

    geometry = Geometry([Autd3([0.0, 0.0, 0.0])])

    async with await autd3.Client.open(
        geometry,
        autd3.TransportOption(),
        autd3.ClientConfig(),
    ) as client:
        print("devices:", client.num_devices())

        center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
        foci = [
            holo.AmplitudeTarget(center + offset(-30.0 * mm, 0.0 * mm, 0.0 * mm), 2.5e3 * Pa),
            holo.AmplitudeTarget(center + offset(30.0 * mm, 0.0 * mm, 0.0 * mm), 2.5e3 * Pa),
        ]

        wavelength = pattern.wavelength(340 * m / s)
        phases = geometry.phase_buffer()
        intensities = geometry.intensity_buffer()
        holo.gspat(geometry, foci, wavelength, holo.GspatOption(), phases, intensities)

        mod_buf = modulation.modulation_buffer()
        modulation.sine(200 * Hz, modulation.SineOption(), mod_buf)

        await client.send(SetSilencer())
        await client.send(Pattern(phases, intensities))
        await client.send(Modulation(SamplingConfig.FREQ_4K, mod_buf))

        print("emitting two GS-PAT foci with a 200 Hz AM — press Ctrl+C to stop")
        stop = asyncio.Event()
        loop = asyncio.get_running_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(sig, stop.set)
        await stop.wait()

        await client.silent_stop()


if __name__ == "__main__":
    asyncio.run(main())
