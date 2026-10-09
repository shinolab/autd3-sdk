"""
Single focus with a 200 Hz sine AM. 

Run with: cargo xtask py example focus_sine
"""

import asyncio
import signal

import autd3
import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3.commands import Modulation, Pattern, SetSilencer
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, m, mm, s
from autd3.value import Intensity, SamplingConfig


async def main() -> None:
    _log_guard = autd3.init_tracing()

    geometry = Geometry([Autd3([0.0, 0.0, 0.0])])

    async with await autd3.Client.open(
        geometry,
        autd3.TransportOption(),
        autd3.ClientConfig(),
    ) as client:
        print("devices:", client.num_devices())
        for i, fw in enumerate(await client.read_firmware_version()):
            print(f"device[{i}] firmware version: {fw}")

        # length in mm
        target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
        wavelength = pattern.wavelength(340 * m / s)
        phases = geometry.phase_buffer()
        pattern.focus(geometry, target, wavelength, phases)

        mod_buf = modulation.modulation_buffer()
        modulation.sine(200 * Hz, modulation.SineOption(), mod_buf)

        await client.send(SetSilencer())
        await client.send(Pattern(phases, Intensity.MAX))
        await client.send(Modulation(SamplingConfig.FREQ_4K, mod_buf))

        print(
            f"emitting a 200 Hz AM focus at "
            f"({target[0]:.2f}, {target[1]:.2f}, {target[2]:.2f}) mm — press Ctrl+C to stop"
        )
        stop = asyncio.Event()
        loop = asyncio.get_running_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(sig, stop.set)
        await stop.wait()

        await client.silent_stop()


if __name__ == "__main__":
    asyncio.run(main())
