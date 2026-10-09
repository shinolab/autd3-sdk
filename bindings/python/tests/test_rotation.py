"""Rotation specification tests: raw quaternion, EulerAngles and scipy interop."""

import subprocess
import sys
from typing import Any

import numpy as np
import numpy.typing as npt
import pytest

from autd3.geometry import Autd3, EulerAngles, Geometry
from autd3.units import deg, rad

ORDERS = ["XYZ", "XZY", "YXZ", "YZX", "ZXY", "ZYX", "XYX", "XZX", "YXY", "YZY", "ZXZ", "ZYZ"]
ANGLES_DEG = (10.0, 20.0, 30.0)


def loose(value: object) -> Any:
    return value


def rotation_of(rotation: Any) -> npt.NDArray[np.float32]:
    return Geometry([Autd3([0.0, 0.0, 0.0], rotation)])[0].rotation()


def axis_quat(axis: str, radian: float) -> npt.NDArray[np.float64]:
    q = np.zeros(4)
    q[0] = np.cos(radian / 2.0)
    q[1 + "XYZ".index(axis)] = np.sin(radian / 2.0)
    return q


def quat_mul(a: npt.NDArray[np.float64], b: npt.NDArray[np.float64]) -> npt.NDArray[np.float64]:
    w1, x1, y1, z1 = a
    w2, x2, y2, z2 = b
    return np.array(
        [
            w1 * w2 - x1 * x2 - y1 * y2 - z1 * z2,
            w1 * x2 + x1 * w2 + y1 * z2 - z1 * y2,
            w1 * y2 - x1 * z2 + y1 * w2 + z1 * x2,
            w1 * z2 + x1 * y2 - y1 * x2 + z1 * w2,
        ]
    )


def assert_same_rotation(a: Any, b: Any) -> None:
    a = np.asarray(a, dtype=float)
    b = np.asarray(b, dtype=float)
    assert np.allclose(a, b, atol=1e-5) or np.allclose(a, -b, atol=1e-5)


def test_raw_quaternion_scalar_first() -> None:
    assert_same_rotation(rotation_of([1.0, 0.0, 0.0, 0.0]), [1.0, 0.0, 0.0, 0.0])
    q = axis_quat("Y", np.pi / 2.0)
    assert_same_rotation(rotation_of(list(q)), q)


@pytest.mark.parametrize("order", ORDERS)
def test_euler_angles_intrinsic(order: str) -> None:
    a1, a2, a3 = ANGLES_DEG
    actual = rotation_of(getattr(EulerAngles, order)(a1 * deg, a2 * deg, a3 * deg))
    expected = quat_mul(
        quat_mul(axis_quat(order[0], np.radians(a1)), axis_quat(order[1], np.radians(a2))),
        axis_quat(order[2], np.radians(a3)),
    )
    assert_same_rotation(actual, expected)


def test_euler_angles_rad_units() -> None:
    assert_same_rotation(
        rotation_of(EulerAngles.ZYZ(0.0 * rad, np.pi / 2.0 * rad, 0.0 * rad)),
        rotation_of(EulerAngles.ZYZ(0.0 * deg, 90.0 * deg, 0.0 * deg)),
    )


def test_scipy_rotation_equivalence() -> None:
    transform = pytest.importorskip("scipy.spatial.transform")
    a1, a2, a3 = ANGLES_DEG
    r = transform.Rotation.from_euler("ZYZ", [a1, a2, a3], degrees=True)
    assert_same_rotation(
        rotation_of(r),
        rotation_of(EulerAngles.ZYZ(a1 * deg, a2 * deg, a3 * deg)),
    )


def test_invalid_rotation_raises() -> None:
    with pytest.raises(ValueError, match="scalar-first"):
        Autd3([0.0, 0.0, 0.0], loose("not a rotation"))


def test_an_omitted_rotation_is_the_identity() -> None:
    assert_same_rotation(Geometry([Autd3([0.0, 0.0, 0.0])])[0].rotation(), [1.0, 0.0, 0.0, 0.0])
    assert_same_rotation(rotation_of(None), [1.0, 0.0, 0.0, 0.0])
    assert Autd3([1.0, 2.0, 3.0]) == Autd3([1.0, 2.0, 3.0], [1.0, 0.0, 0.0, 0.0])
    assert Autd3([1.0, 2.0, 3.0]) != Autd3([0.0, 2.0, 3.0])
    assert Autd3([1.0, 2.0, 3.0]).origin.tolist() == [1.0, 2.0, 3.0]


@pytest.mark.parametrize(
    "quaternion",
    [
        [2.0, 0.0, 0.0, 0.0],
        [1.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [1.002, 0.0, 0.0, 0.0],
        [0.998, 0.0, 0.0, 0.0],
        [float("nan"), 0.0, 0.0, 0.0],
        [float("inf"), 0.0, 0.0, 0.0],
    ],
)
def test_a_non_unit_quaternion_is_rejected(quaternion: list[float]) -> None:
    with pytest.raises(ValueError, match="unit quaternion"):
        Autd3([0.0, 0.0, 0.0], quaternion)


@pytest.mark.parametrize("quaternion", [[1.0005, 0.0, 0.0, 0.0], [0.9995, 0.0, 0.0, 0.0]])
def test_a_quaternion_within_the_tolerance_is_normalized(quaternion: list[float]) -> None:
    assert_same_rotation(rotation_of(quaternion), [1.0, 0.0, 0.0, 0.0])


def test_import_does_not_load_scipy() -> None:
    code = "import autd3_core, sys; assert 'scipy' not in sys.modules"
    subprocess.run([sys.executable, "-c", code], check=True)
