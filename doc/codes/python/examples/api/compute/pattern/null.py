from autd3.geometry import Autd3, Geometry
from autd3_pattern import null

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

out = geometry.pattern_buffer()

# ANCHOR: api
null(out)
# ANCHOR_END: api
