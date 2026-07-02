from autd3.geometry import Autd3, Geometry
from autd3_pattern import null

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

out = geometry.pattern_buffer()

null(out)
