"""Hardware-free tests: client configuration and the mutable pattern capsule."""

import gc

import numpy as np
import pytest

import autd3
import autd3_pattern as pattern
import autd3_pattern_holo as holo
from autd3_pattern_holo import Pa


def geometry() -> autd3.geometry.Geometry:
    return autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])


def test_rt_schedule_policy_is_settable() -> None:
    for policy in (
        autd3.RtSchedulePolicy.Normal,
        autd3.RtSchedulePolicy.Fifo,
        autd3.RtSchedulePolicy.RoundRobin,
    ):
        autd3.ClientConfig(rt_policy=policy)
        autd3.LegacyClientConfig(rt_policy=policy)


def test_require_supported_firmware_is_settable() -> None:
    autd3.ClientConfig(require_supported_firmware=True)
    autd3.ClientConfig(require_supported_firmware=False)


def test_rt_priority_and_disable_are_mutually_exclusive() -> None:
    with pytest.raises(ValueError):
        autd3.ClientConfig(rt_priority=49, disable_rt_priority=True)
    with pytest.raises(ValueError):
        autd3.LegacyClientConfig(rt_priority=49, disable_rt_priority=True)


def test_zero_valued_config_fields_are_rejected() -> None:
    with pytest.raises(ValueError):
        autd3.ClientConfig(timeout_cycles=0)
    with pytest.raises(ValueError):
        autd3.ClientConfig(max_inflight=0)
    with pytest.raises(ValueError):
        autd3.LegacyClientConfig(timeout_cycles=0)


def test_the_mutable_capsule_keeps_its_buffer_alive() -> None:
    geo = geometry()
    buffer = geo.pattern_buffer()
    capsule = buffer._capsule_mut()
    del buffer
    gc.collect()

    holo.naive(
        geo,
        [holo.AmplitudeTarget(np.array([0.0, 0.0, 150.0]), 5e3 * Pa)],
        pattern.wavelength(340 * autd3.units.m / autd3.units.s),
        holo.NaiveOption(),
        capsule,
    )
