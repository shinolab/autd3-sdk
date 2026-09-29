import runpy
import sys

import autd3

_emulators = []
_orig_open = autd3.Client.open


def _emulated(geometry):
    emulator = autd3.UdpEmulator(geometry.num_devices())
    _emulators.append(emulator)
    return emulator.option()


def _patched_open(geometry, option, config):
    return _orig_open(geometry, _emulated(geometry), config)

autd3.Client.open = staticmethod(_patched_open)

if hasattr(autd3.Client, "open_with_checker"):
    _orig_open_with_checker = autd3.Client.open_with_checker

    def _patched_open_with_checker(geometry, option, config):
        return _orig_open_with_checker(geometry, _emulated(geometry), config)

    autd3.Client.open_with_checker = staticmethod(_patched_open_with_checker)

runpy.run_path(sys.argv[1], run_name="__main__")
