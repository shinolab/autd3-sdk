from pathlib import Path

import numpy as np
import pytest

from autd3.geometry import Autd3, Geometry

FIXTURE = """[
  { "origin": [0.0, 0.0, 0.0] },
  { "origin": [192.0, 0.0, 0.0], "rotation": [0.7071068, 0.0, 0.7071068, 0.0] }
]"""

PITCH_MM = 10.16


def two_devices() -> Geometry:
    return Geometry(
        [
            Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
            Autd3([192.0, 0.0, 0.0], [0.7071068, 0.0, 0.7071068, 0.0]),
        ]
    )


def test_from_json_builds_the_declared_placement() -> None:
    geometry = Geometry.from_json(FIXTURE)
    assert len(geometry) == 2
    assert np.allclose(geometry[0].position(0), [0.0, 0.0, 0.0])
    assert np.allclose(geometry[1].position(0), [192.0, 0.0, 0.0])
    assert np.allclose(geometry[1].position(1), [192.0, 0.0, -PITCH_MM], atol=1e-3)
    assert np.allclose(geometry[1].axial_direction(), [1.0, 0.0, 0.0], atol=1e-3)


def test_to_json_round_trips_through_from_json() -> None:
    geometry = Geometry.from_json(two_devices().to_json())
    assert len(geometry) == 2
    assert np.allclose(geometry[1].position(0), [192.0, 0.0, 0.0])
    assert np.allclose(geometry[1].axial_direction(), [1.0, 0.0, 0.0], atol=1e-3)


def test_to_json_round_trips_through_a_file(tmp_path: Path) -> None:
    path = tmp_path / "layout.json"
    path.write_text(two_devices().to_json())

    geometry = Geometry.from_json(path.read_text())
    assert len(geometry) == 2
    assert np.allclose(geometry[1].position(0), [192.0, 0.0, 0.0])


def test_rotation_defaults_to_the_identity_quaternion() -> None:
    geometry = Geometry.from_json('[{"origin": [0, 0, 0]}]')
    assert np.allclose(geometry[0].rotation(), [1.0, 0.0, 0.0, 0.0])


@pytest.mark.parametrize(
    ("text", "message"),
    [
        ('[{"origin": [0,0,0], "rotaton": [1,0,0,0]}]', "rotaton"),
        ('[{"origin": [0,0,0], "rotation": [1,1,0,0]}]', "unit quaternion"),
        ('[{"origin": [0,0]}]', "length"),
    ],
)
def test_an_invalid_layout_is_rejected(text: str, message: str) -> None:
    with pytest.raises(Exception, match=message):
        Geometry.from_json(text)

