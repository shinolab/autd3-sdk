from autd3.commands import SetOutputMask
from autd3.geometry import Autd3, Geometry

geometry = Geometry([Autd3()])

masks = geometry.output_mask_buffer()

# ANCHOR: api
SetOutputMask(masks)
# ANCHOR_END: api
