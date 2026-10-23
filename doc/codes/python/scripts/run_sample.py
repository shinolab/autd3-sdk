import atexit
import os
import runpy
import subprocess
import sys

import autd3

_udp_emulator = os.environ["AUTD3_UDP_EMULATOR"]
_orig_client_open = autd3.Client.open
_emulators = []


def _stop_emulators():
    for process in _emulators:
        if process.stdin is not None:
            process.stdin.close()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


atexit.register(_stop_emulators)


def _spawn_emulator(num_devices):
    process = subprocess.Popen(
        [_udp_emulator, "--devices", str(num_devices)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        text=True,
    )
    _emulators.append(process)
    addr = process.stdout.readline().strip()
    if not addr:
        raise RuntimeError(f"the UDP emulator exited without an address: {process.wait()}")
    return addr


def _patched_client_open(geometry, option):
    addr = _spawn_emulator(geometry.num_devices())
    return _orig_client_open(geometry, autd3.TransportOption(iface=autd3.Interface.Addr(addr)))


autd3.Client.open = staticmethod(_patched_client_open)

runpy.run_path(sys.argv[1], run_name="__main__")
