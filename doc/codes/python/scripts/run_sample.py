import os
import runpy
import socket
import struct
import sys

import autd3

_SIMULATOR_CONTROL_VERSION = 1
_SIMULATOR_STATUS_OK = 0
_SIMULATOR_REPLY_TIMEOUT_S = 10.0

_simulator_port = int(os.environ["AUTD3_SIMULATOR_PORT"])
_simulator_option = autd3.TransportOption(iface=autd3.Interface.Addr(f"[::1]:{_simulator_port}"))
_orig_client_open = autd3.Client.open


def _send_geometry_to_simulator(geometry):
    message = bytearray([_SIMULATOR_CONTROL_VERSION])
    for device in geometry:
        origin = device.position(0)
        rotation = device.rotation()
        message += struct.pack("<7f", *origin, *rotation)
    control = ("::1", _simulator_port + 1)
    with socket.socket(socket.AF_INET6, socket.SOCK_DGRAM) as sock:
        sock.bind(("::1", 0))
        sock.settimeout(_SIMULATOR_REPLY_TIMEOUT_S)
        sock.sendto(bytes(message), control)
        while True:
            reply, src = sock.recvfrom(16)
            if src[:2] != control or len(reply) != 2:
                continue
            version, status = reply
            if version != _SIMULATOR_CONTROL_VERSION or status != _SIMULATOR_STATUS_OK:
                raise RuntimeError(f"the simulator refused the geometry: version {version}, status {status}")
            return


def _patched_client_open(geometry, option):
    _send_geometry_to_simulator(geometry)
    return _orig_client_open(geometry, _simulator_option)


autd3.Client.open = staticmethod(_patched_client_open)

runpy.run_path(sys.argv[1], run_name="__main__")
