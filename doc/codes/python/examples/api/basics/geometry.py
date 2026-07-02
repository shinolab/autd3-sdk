from autd3.geometry import Autd3, Geometry

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
