from autd3.commands import SetPhaseCorrection
from autd3.geometry import Autd3, Geometry
from autd3.value import Phase

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

phases = [[Phase.ZERO] * geometry.device(i).num_transducers() for i in range(geometry.num_devices())]

# ANCHOR: api
SetPhaseCorrection(phases)
# ANCHOR_END: api
