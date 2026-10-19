import importlib
import sys
import sysconfig
import warnings

import pytest

MODULES = [
    "autd3_core",
    "autd3",
    "autd3_pattern",
    "autd3_pattern_holo",
    "autd3_modulation",
    "autd3_emulator",
]

pytestmark = pytest.mark.skipif(
    not sysconfig.get_config_var("Py_GIL_DISABLED"),
    reason="only meaningful on a free-threaded build",
)


def gil_enabled() -> bool:
    if sys.version_info >= (3, 13):
        return sys._is_gil_enabled()
    return True


@pytest.mark.parametrize("name", MODULES)
def test_importing_a_module_keeps_the_gil_disabled(name: str) -> None:
    with warnings.catch_warnings():
        warnings.simplefilter("error", RuntimeWarning)
        importlib.import_module(name)
    assert not gil_enabled()
