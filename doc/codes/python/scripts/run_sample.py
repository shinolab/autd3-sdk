import os
import runpy
import sys

import autd3

_simulator_option = autd3.TransportOption(iface=autd3.Interface.Simulator(int(os.environ["AUTD3_SIMULATOR_PORT"])))
_orig_client_open = autd3.Client.open


def _patched_client_open(geometry, option):
    return _orig_client_open(geometry, _simulator_option)


autd3.Client.open = staticmethod(_patched_client_open)

runpy.run_path(sys.argv[1], run_name="__main__")
