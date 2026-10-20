from typing import Any

import numpy as np
import pytest

import autd3
from autd3 import MAX_DEVICES, DeviceMask


def loose(value: object) -> Any:
    return value


def test_device_mask_is_exported_from_the_package_root() -> None:
    assert "DeviceMask" in autd3.__all__
    assert DeviceMask.__module__ == "autd3"


def test_indexing_tells_whether_a_device_is_in_the_mask() -> None:
    mask = DeviceMask.from_devices([1, 3, MAX_DEVICES - 1])
    assert mask[0] is False
    assert mask[1] is True
    assert mask[2] is False
    assert mask[3] is True
    assert mask[MAX_DEVICES - 1] is True
    assert mask[127] is True
    assert mask[128] is False
    assert mask[-1] is False
    assert mask[2**200] is False
    assert mask[-(2**200)] is False
    assert mask[np.int64(3)] is True


def test_indexing_with_a_non_integer_is_a_type_error() -> None:
    mask = DeviceMask.from_devices([0])
    with pytest.raises(TypeError):
        loose(mask)["0"]
    with pytest.raises(TypeError):
        loose(mask)[0.0]


def test_len_counts_devices_and_bool_tells_non_empty() -> None:
    empty = DeviceMask.from_devices([])
    assert len(empty) == 0
    assert not empty
    mask = DeviceMask.from_devices([0, 5, 127])
    assert len(mask) == 3
    assert mask


def test_iteration_yields_indices_in_ascending_order() -> None:
    mask = DeviceMask.from_devices([127, 4, 0, 64, 2])
    assert list(mask) == [0, 2, 4, 64, 127]
    assert list(DeviceMask.from_devices([])) == []


def test_membership() -> None:
    mask = DeviceMask.from_devices([0, 127])
    assert 0 in mask
    assert 127 in mask
    assert 1 not in mask
    assert 128 not in mask
    assert -1 not in mask
    assert loose("0") not in mask
    assert loose(None) not in mask


def test_union_joins_masks() -> None:
    lhs = DeviceMask.from_devices([0, 2])
    rhs = DeviceMask.from_devices([2, 127])
    joined = lhs | rhs
    assert isinstance(joined, DeviceMask)
    assert list(joined) == [0, 2, 127]
    assert list(lhs) == [0, 2]
    assert list(rhs) == [2, 127]
    with pytest.raises(TypeError):
        loose(lhs) | 1


def test_from_devices_ignores_out_of_range_and_duplicates() -> None:
    mask = DeviceMask.from_devices([3, 0, 3, -1, 128, 2**200, MAX_DEVICES])
    assert list(mask) == [0, 3]
    assert mask == DeviceMask.from_devices([0, 3])
    assert DeviceMask.from_devices(i for i in (1, 1, 2)) == DeviceMask.from_devices([1, 2])
    with pytest.raises(TypeError):
        DeviceMask.from_devices(loose(["1"]))
    with pytest.raises(TypeError):
        DeviceMask.from_devices(loose(1))


def test_bits_are_the_raw_bit_set() -> None:
    assert DeviceMask.from_devices([]).bits == 0
    assert DeviceMask.from_devices([1, 3]).bits == 0b1010
    assert DeviceMask.from_devices([127]).bits == 1 << 127
    assert DeviceMask.from_devices(range(MAX_DEVICES)).bits == (1 << 128) - 1


def test_equality_and_hash() -> None:
    lhs = DeviceMask.from_devices([1, 3])
    rhs = DeviceMask.from_devices([3, 1, 1])
    other = DeviceMask.from_devices([1])
    assert lhs == rhs
    assert hash(lhs) == hash(rhs)
    assert lhs != other
    assert lhs != loose(0b1010)
    assert len({lhs, rhs, other}) == 2


def test_device_mask_is_immutable() -> None:
    mask = DeviceMask.from_devices([1])
    with pytest.raises(AttributeError):
        loose(mask).bits = 0
    with pytest.raises(TypeError):
        loose(mask)[0] = True


def test_repr_lists_the_devices() -> None:
    assert repr(DeviceMask.from_devices([3, 1])) == "DeviceMask([1, 3])"
    assert repr(DeviceMask.from_devices([])) == "DeviceMask([])"
