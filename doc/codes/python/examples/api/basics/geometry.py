from scipy.spatial.transform import Rotation

from autd3.geometry import Autd3, EulerAngles, Geometry
from autd3.units import deg

# ANCHOR: api
Geometry(
    [
        Autd3(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
        )
    ]
)
# ANCHOR_END: api

# ANCHOR: rotation
Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])
Autd3([0.0, 0.0, 0.0], EulerAngles.ZYZ(0.0 * deg, 90.0 * deg, 0.0 * deg))
Autd3([0.0, 0.0, 0.0], Rotation.from_euler("ZYZ", [0.0, 90.0, 0.0], degrees=True))
# ANCHOR_END: rotation

geometry = Geometry(
    [
        Autd3(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
        )
    ]
)

# ANCHOR: access
num_devices = geometry.num_devices()
total_transducers = geometry.num_transducers()
array_center = geometry.center()

for device in geometry:
    pass

first = geometry[0]
# ANCHOR_END: access

device = geometry[0]
# ANCHOR: device
idx = device.idx()
num_transducers = device.num_transducers()
center = device.center()
rotation = device.rotation()
x = device.x_direction()
y = device.y_direction()
axial = device.axial_direction()
positions = device.positions()
directions = device.directions()
pos0 = device.position(0)
dir0 = device.direction(0)
# ANCHOR_END: device
