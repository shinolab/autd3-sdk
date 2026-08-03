"""
Remote Link client: connects to a remote server over TCP and emits a 200 Hz sine AM focus

Start the Rust `remote_server` example (or the simulator) first.
Pass an address to skip the mDNS lookup; without one it falls back to the local default
when no appliance answers, which is where remote_server and the simulator both listen.

Run with: cargo xtask py example remote_client
"""

import asyncio
import signal
import sys

import numpy as np

import autd3
import autd3_link_remote as remote
import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3.units import Hz, m, s

LOCAL_ADDR = "127.0.0.1:8080"


def link_option() -> remote.RemoteLinkOption:
    if len(sys.argv) > 1:
        return remote.RemoteLinkOption(sys.argv[1])
    try:
        return remote.RemoteLinkOption.discover()
    except ValueError as e:
        print(f"discovery found no appliance ({e}); falling back to {LOCAL_ADDR}")
        return remote.RemoteLinkOption(LOCAL_ADDR)


async def main() -> None:
    option = link_option()
    addr = option.addr
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    client = await autd3.Client.open(
        geometry,
        option,
        autd3.ClientConfig(),
    )

    print(f"connected to {addr}, devices:", client.num_devices())
    for i, fw in enumerate(await client.read_firmware_version()):
        print(f"device[{i}] firmware version: {fw}")

    target = geometry.center() + np.array([0.0, 0.0, 150.0])
    wavelength = pattern.wavelength(340 * m / s)
    patterns = geometry.pattern_buffer()
    pattern.focus(geometry, target, wavelength, pattern.FocusOption(), patterns)

    mod_buf = modulation.modulation_buffer()
    modulation.sine(200 * Hz, modulation.SineOption(), mod_buf)

    builder = client.datagram_builder()
    builder.push(autd3.commands.SetSilencer())
    builder.push(autd3.commands.Pattern(patterns))
    builder.push(autd3.commands.Modulation(autd3.value.SamplingConfig.FREQ_4K, mod_buf))
    for frame in builder.build():
        await client.send_checked(frame)

    print("emitting a 200 Hz AM focus over the network — press Ctrl+C to stop")
    stop = asyncio.Event()
    loop = asyncio.get_running_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, stop.set)
    await stop.wait()

    await client.stop()
    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
