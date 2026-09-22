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


def test_rt_priority_accepts_a_priority_or_none() -> None:
    for priority in (autd3.RtPriority(49), autd3.RtPriority.MIN, autd3.RtPriority.MAX, None):
        autd3.ClientConfig(rt_priority=priority)
        autd3.LegacyClientConfig(rt_priority=priority)


def test_rt_priority_rejects_an_out_of_range_value() -> None:
    with pytest.raises(ValueError):
        autd3.RtPriority(100)


def test_rt_priority_exposes_its_value() -> None:
    assert autd3.RtPriority(49).value == 49
    assert autd3.RtPriority(49) == autd3.RtPriority(49)
    assert repr(autd3.RtPriority(49)) == "RtPriority(49)"
    assert autd3.RtPriority.MIN.value is None
    assert repr(autd3.RtPriority.MAX) == "RtPriority.MAX"


def test_zero_valued_config_fields_are_rejected() -> None:
    with pytest.raises(ValueError):
        autd3.ClientConfig(timeout_cycles=0)
    with pytest.raises(ValueError):
        autd3.ClientConfig(max_inflight=0)
    with pytest.raises(ValueError):
        autd3.LegacyClientConfig(timeout_cycles=0)


def test_the_mutable_capsule_keeps_its_buffer_alive() -> None:
    geo = geometry()
    phases = geo.phase_buffer()
    intensities = geo.intensity_buffer()
    phase_capsule = phases._capsule_mut()
    intensity_capsule = intensities._capsule_mut()
    del phases
    del intensities
    gc.collect()

    holo.naive(
        geo,
        [holo.AmplitudeTarget(np.array([0.0, 0.0, 150.0]), 5e3 * Pa)],
        pattern.wavelength(340 * autd3.units.m / autd3.units.s),
        holo.NaiveOption(),
        phase_capsule,
        intensity_capsule,
    )
