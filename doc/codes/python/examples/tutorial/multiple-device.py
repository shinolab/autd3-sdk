import math

import autd3


def main() -> None:
    # ANCHOR: translation
    autd3.geometry.Geometry(
        [
            autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
            autd3.geometry.Autd3(
                origin=(autd3.geometry.Autd3.DEVICE_WIDTH, 0.0, 0.0),
                rotation=(1.0, 0.0, 0.0, 0.0),
            ),
        ]
    )
    # ANCHOR_END: translation

    # ANCHOR: global
    autd3.geometry.Geometry(
        [
            autd3.geometry.Autd3(
                origin=(-autd3.geometry.Autd3.DEVICE_WIDTH, 0.0, 0.0),
                rotation=(1.0, 0.0, 0.0, 0.0),
            ),
            autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
        ]
    )
    # ANCHOR_END: global

    # ANCHOR: rotation
    autd3.geometry.Geometry(
        [
            autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
            autd3.geometry.Autd3(
                origin=(0.0, 0.0, autd3.geometry.Autd3.DEVICE_WIDTH),
                rotation=(math.cos(math.pi / 4), 0.0, math.sin(math.pi / 4), 0.0),
            ),
        ]
    )
    # ANCHOR_END: rotation


if __name__ == "__main__":
    main()
