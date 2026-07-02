import asyncio

import numpy as np

import autd3
import autd3_link_ethercrab as ethercrab
import autd3_pattern as pattern
from autd3.units import m, s


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    client = await autd3.Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        autd3.ClientConfig(),
    )

    wavelength = pattern.wavelength(340 * m / s)

    # ANCHOR: switch
    # Write focus A to bank B0 and play it.
    target_a = geometry.center() + np.array([0.0, 0.0, 150.0])
    pat_a = geometry.pattern_buffer()
    pattern.focus(
        geometry,
        target_a,
        wavelength,
        pattern.FocusOption(),
        pat_a,
    )
    builder = client.datagram_builder()
    builder.push(autd3.commands.Pattern(pat_a, bank=autd3.value.PatternBank.B0))
    for frame in builder.build():
        await client.send_checked(frame)

    # Write focus B to bank B1, which is not currently playing, then switch to B1.
    # B0 keeps playing cleanly while B1 is being written (double buffering).
    target_b = geometry.center() + np.array([0.0, 30.0, 150.0])
    pat_b = geometry.pattern_buffer()
    pattern.focus(
        geometry,
        target_b,
        wavelength,
        pattern.FocusOption(),
        pat_b,
    )
    builder = client.datagram_builder()
    builder.push(autd3.commands.Pattern(pat_b, bank=autd3.value.PatternBank.B1))
    for frame in builder.build():
        await client.send_checked(frame)
    # ANCHOR_END: switch

    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
