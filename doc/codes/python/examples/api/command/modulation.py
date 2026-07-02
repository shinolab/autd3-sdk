from autd3.commands import ChangeModulationBank, ConfigModulation, Modulation, WriteModulationBuffer
from autd3.value import LoopBehavior, ModulationBank, SamplingConfig, TransitionMode
from autd3_modulation import modulation_buffer

bank = ModulationBank.B0
config = SamplingConfig.FREQ_4K
loop_behavior = LoopBehavior.Infinite
transition_mode = TransitionMode.Immediate

data = modulation_buffer()

# ANCHOR: api
Modulation(config, data)

Modulation(
    bank=bank,
    config=config,
    data=data,
    loop_behavior=loop_behavior,
    transition_mode=transition_mode,
)
# ANCHOR_END: api

# ANCHOR: equivalent
WriteModulationBuffer(
    bank=bank,
    offset=0,
    data=data,
)
ConfigModulation(
    bank=bank,
    config=config,
    size=len(data),
    loop_behavior=loop_behavior,
)
ChangeModulationBank(
    bank=bank,
    transition_mode=transition_mode,
)
# ANCHOR_END: equivalent
