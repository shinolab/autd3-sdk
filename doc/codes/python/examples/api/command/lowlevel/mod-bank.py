from autd3.commands import ChangeModulationBank, ConfigModulation, WriteModulationBuffer
from autd3.units import Hz
from autd3.value import LoopBehavior, ModulationBank, SamplingConfig, TransitionMode
from autd3_modulation import SineOption, modulation_buffer, sine

bank = ModulationBank.B0
offset = 0
buffer = modulation_buffer()
sine(150.0 * Hz, SineOption(), buffer)
data = buffer
# ANCHOR: write
WriteModulationBuffer(bank, offset, data)
# ANCHOR_END: write
config = SamplingConfig.FREQ_4K
size = len(data)
loop_behavior = LoopBehavior.Infinite
# ANCHOR: config
ConfigModulation(
    bank,
    config,
    size,
    loop_behavior,
)
# ANCHOR_END: config
transition_mode = TransitionMode.Immediate
# ANCHOR: change
ChangeModulationBank(
    bank,
    transition_mode,
)
# ANCHOR_END: change
