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


def test_require_supported_firmware_is_settable() -> None:
    autd3.ClientConfig(require_supported_firmware=True)
    autd3.ClientConfig(require_supported_firmware=False)


def test_rt_tuning_is_no_longer_part_of_the_config() -> None:
    assert not hasattr(autd3, "RtPriority")
    assert not hasattr(autd3, "RtSchedulePolicy")
    with pytest.raises(TypeError):
        autd3.ClientConfig(rt_priority=None)


def test_zero_valued_config_fields_are_rejected() -> None:
    with pytest.raises(ValueError):
        autd3.ClientConfig(ack_timeout=autd3.Duration.from_millis(0))
    with pytest.raises(ValueError):
        autd3.ClientConfig(max_inflight=0)


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
