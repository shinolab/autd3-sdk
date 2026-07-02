import runpy
import sys

import autd3
import autd3_link_nop as _nop

_orig_open = autd3.Client.open

def _patched_open(geometry, link, config):
    return _orig_open(geometry, _nop.Nop(), config)

autd3.Client.open = staticmethod(_patched_open)

if hasattr(autd3.Client, "open_with_checker"):
    _orig_open_with_checker = autd3.Client.open_with_checker

    def _patched_open_with_checker(geometry, link, config):
        return _orig_open_with_checker(geometry, _nop.Nop(), config)

    autd3.Client.open_with_checker = staticmethod(_patched_open_with_checker)

runpy.run_path(sys.argv[1], run_name="__main__")
