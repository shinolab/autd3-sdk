from autd3.commands import ActivateModulationBank, ConfigModulation, Modulation, WriteModulationBuffer
from autd3.value import LoopBehavior, ModulationBank, SamplingConfig, TransitionMode
from autd3_modulation import ModulationBuffer, modulation_buffer

bank = ModulationBank.B0
config = SamplingConfig.FREQ_4K
loop_behavior = LoopBehavior.Infinite
transition_mode = TransitionMode.Immediate

data = modulation_buffer()

# ANCHOR: api
Modulation(config, data)

Modulation(config, data, bank=bank)

Modulation(
    bank=bank,
    config=config,
    data=data,
    loop_behavior=loop_behavior,
    transition_mode=transition_mode,
)
# ANCHOR_END: api

# ANCHOR: equivalent
samples = data.to_numpy()
for offset in range(0, len(samples), WriteModulationBuffer.MAX_LEN):
    WriteModulationBuffer(
        bank=bank,
        offset=offset,
        data=ModulationBuffer.from_array(samples[offset : offset + WriteModulationBuffer.MAX_LEN]),
    )
ConfigModulation(
    bank=bank,
    config=config,
    size=len(data),
    loop_behavior=loop_behavior,
)
ActivateModulationBank(
    bank=bank,
    transition_mode=transition_mode,
)
# ANCHOR_END: equivalent
