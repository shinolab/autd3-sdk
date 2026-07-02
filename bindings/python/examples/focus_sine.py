"""Single focus with a 200 Hz sine AM. Run with: cargo xtask py example focus_sine"""

import asyncio
import signal

import numpy as np

import autd3
import autd3_link_ethercrab as ethercrab
import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3.units import Hz, m, s


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    client = await autd3.Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        autd3.ClientConfig(),
    )

    print("devices:", client.num_devices())
    for i, fw in enumerate(await client.read_firmware_version()):
        print(f"device[{i}] firmware version: {fw}")

    # length in mm; sound speed as a Velocity
    target = geometry.center() + np.array([0.0, 0.0, 150.0])
    wavelength = pattern.wavelength(340 * m / s)
    patterns = geometry.pattern_buffer()
    pattern.focus(geometry, target, wavelength, pattern.FocusOption(), patterns)

    mod_buf = modulation.modulation_buffer()
    modulation.sine(200 * Hz, modulation.SineOption(), mod_buf)

    builder = client.datagram_builder()
    builder.push(autd3.commands.Pattern(patterns))
    builder.push(autd3.commands.Modulation(autd3.value.SamplingConfig.FREQ_4K, mod_buf))
    datagrams = builder.build()
    for frame in datagrams:
        await client.send_checked(frame)

    print(
        f"emitting a 200 Hz AM focus at "
        f"({target[0]:.2f}, {target[1]:.2f}, {target[2]:.2f}) mm — press Ctrl+C to stop"
    )
    stop = asyncio.Event()
    loop = asyncio.get_running_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, stop.set)
    await stop.wait()

    await client.stop()
    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
