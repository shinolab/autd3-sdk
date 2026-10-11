import asyncio

from autd3 import Client, ClientConfig, UdpEmulator
from autd3.commands import ActivateModulationBank, ConfigModulation, WriteModulationBuffer
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz
from autd3.value import LoopBehavior, ModulationBank, SamplingConfig, TransitionMode
from autd3_modulation import SineOption, modulation_buffer, sine


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    emulator = UdpEmulator(geometry.num_devices())
    async with await Client.open(geometry, emulator.option(), ClientConfig()) as client:
        data = modulation_buffer()
        sine(150 * Hz, SineOption(), data)

        bank = ModulationBank.B0

        await client.send(
            WriteModulationBuffer(
                bank=bank,
                offset=0,
                data=data,
            )
        )
        await client.send(
            ConfigModulation(
                bank=bank,
                config=SamplingConfig.FREQ_4K,
                size=len(data),
                loop_behavior=LoopBehavior.Infinite,
            )
        )
        await client.send(
            ActivateModulationBank(
                bank=bank,
                transition_mode=TransitionMode.Immediate,
            )
        )


asyncio.run(main())
