from autd3.commands import SetPhaseCorrection
from autd3.geometry import Autd3, Geometry

geometry = Geometry([Autd3()])

phases = geometry.phase_buffer()

# ANCHOR: api
SetPhaseCorrection(phases)
# ANCHOR_END: api
