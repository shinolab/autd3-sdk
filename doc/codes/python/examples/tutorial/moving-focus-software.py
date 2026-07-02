import asyncio

import numpy as np

import autd3
import autd3_link_ethercrab as ethercrab
import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3.units import m, s

# xtask:long-running


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    client = await autd3.Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        autd3.ClientConfig(),
    )

    center = geometry.center() + np.array([0.0, 0.0, 150.0])
    wavelength = pattern.wavelength(340 * m / s)

    # ANCHOR: loop
    patterns = geometry.pattern_buffer()
    while True:
        for sign in (1.0, -1.0):
            target = center + np.array([sign * 20.0, 0.0, 0.0])
            pattern.focus(
                geometry,
                target,
                wavelength,
                pattern.FocusOption(),
                patterns,
            )
            builder = client.datagram_builder()
            builder.push(autd3.commands.Pattern(patterns))
            for frame in builder.build():
                await client.send_checked(frame)
            await asyncio.sleep(1.0)
    # ANCHOR_END: loop


if __name__ == "__main__":
    asyncio.run(main())
