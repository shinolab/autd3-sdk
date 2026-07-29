import asyncio

import numpy as np

import autd3_link_echocat as echocat
import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3 import LegacyChangePatternBank, LegacyClient, LegacyClientConfig
from autd3.commands import ChangeModulationBank, Modulation, Pattern, SetSilencer
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz, m, s
from autd3.value import ModulationBank, PatternBank, SamplingConfig, TransitionMode


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    # ANCHOR: open
    client = await LegacyClient.open(
        geometry,
        echocat.EchocatLinkOption(),
        LegacyClientConfig(),
    )
    # ANCHOR_END: open

    # ANCHOR: version
    for i, version in enumerate(await client.read_firmware_version()):
        print(f"device[{i}] firmware version: {version}")
    # ANCHOR_END: version

    target = geometry.center() + np.array([0.0, 0.0, 150.0])
    emissions = geometry.pattern_buffer()
    pattern.focus(geometry, target, pattern.wavelength(340 * m / s), pattern.FocusOption(), emissions)
    mod_buf = modulation.modulation_buffer()
    modulation.sine(200 * Hz, modulation.SineOption(), mod_buf)

    # ANCHOR: send
    builder = client.datagram_builder()
    builder.push(SetSilencer())
    builder.push(Pattern(emissions))
    builder.push(Modulation(SamplingConfig.FREQ_4K, mod_buf))
    frames = builder.build()
    for i in range(len(frames)):
        await client.send_checked(frames[i])
    # ANCHOR_END: send

    # ANCHOR: change_bank
    builder = client.datagram_builder()
    builder.push(Pattern(emissions, PatternBank.B1))
    frames = builder.build()
    for i in range(len(frames)):
        await client.send_checked(frames[i])

    builder = client.datagram_builder()
    builder.push(LegacyChangePatternBank.pattern(PatternBank.B0))
    frames = builder.build()
    for i in range(len(frames)):
        await client.send_checked(frames[i])
    # ANCHOR_END: change_bank

    # ANCHOR: later
    builder = client.datagram_builder()
    builder.push(
        Modulation(
            SamplingConfig.FREQ_4K,
            mod_buf,
            bank=ModulationBank.B1,
            transition_mode=TransitionMode.Later,
        )
    )
    frames = builder.build()
    for i in range(len(frames)):
        await client.send_checked(frames[i])

    builder = client.datagram_builder()
    builder.push(ChangeModulationBank(ModulationBank.B1, transition_mode=TransitionMode.Immediate))
    frames = builder.build()
    for i in range(len(frames)):
        await client.send_checked(frames[i])
    # ANCHOR_END: later

    # ANCHOR: close
    await client.stop()
    await client.close()
    # ANCHOR_END: close


if __name__ == "__main__":
    asyncio.run(main())
