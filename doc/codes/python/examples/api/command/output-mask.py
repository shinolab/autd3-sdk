from autd3.commands import SetOutputMask
from autd3.geometry import Autd3, Geometry

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

masks = [[True] * geometry.device(i).num_transducers() for i in range(geometry.num_devices())]

# ANCHOR: api
SetOutputMask(masks)
# ANCHOR_END: api
