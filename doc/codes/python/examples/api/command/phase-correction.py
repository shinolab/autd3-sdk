from autd3.commands import SetPhaseCorrection
from autd3.geometry import Autd3, Geometry
from autd3.value import Phase

geometry = Geometry([Autd3()])

phases = [[Phase.ZERO] * device.num_transducers() for device in geometry]

# ANCHOR: api
SetPhaseCorrection(phases)
# ANCHOR_END: api
