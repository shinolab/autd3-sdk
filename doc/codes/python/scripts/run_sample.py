import runpy
import sys

import autd3

_emulators = []
_orig_client_open = autd3.Client.open


def _patched_client_open(geometry, option, config):
    emulator = autd3.UdpEmulator(geometry.num_devices())
    _emulators.append(emulator)
    return _orig_client_open(geometry, emulator.option(), config)


autd3.Client.open = staticmethod(_patched_client_open)

runpy.run_path(sys.argv[1], run_name="__main__")
