from scipy.spatial.transform import Rotation

from autd3.geometry import Autd3, Geometry

def main() -> None:
    # ANCHOR: translation
    Geometry(
        [
            Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
            Autd3(
                origin=(Autd3.DEVICE_WIDTH, 0.0, 0.0),
                rotation=(1.0, 0.0, 0.0, 0.0),
            ),
        ]
    )
    # ANCHOR_END: translation

    # ANCHOR: global
    Geometry(
        [
            Autd3(
                origin=(Autd3.DEVICE_WIDTH, 0.0, 0.0),
                rotation=(1.0, 0.0, 0.0, 0.0),
            ),
            Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
        ]
    )
    # ANCHOR_END: global

    # ANCHOR: rotation
    Geometry(
        [
            Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
            Autd3(
                origin=(0.0, 0.0, Autd3.DEVICE_WIDTH),
                rotation=Rotation.from_euler("y", 90.0, degrees=True),
            ),
        ]
    )
    # ANCHOR_END: rotation


if __name__ == "__main__":
    main()
