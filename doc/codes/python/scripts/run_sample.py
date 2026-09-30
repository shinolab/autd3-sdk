import runpy
import sys

import autd3

_emulators = []
_orig_driver_open = autd3.Driver.open


def _patched_driver_open(option, num_devices):
    emulator = autd3.UdpEmulator(num_devices)
    _emulators.append(emulator)
    return _orig_driver_open(emulator.option(), num_devices)


autd3.Driver.open = staticmethod(_patched_driver_open)

runpy.run_path(sys.argv[1], run_name="__main__")
