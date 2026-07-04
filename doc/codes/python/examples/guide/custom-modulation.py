from autd3.commands import Modulation
from autd3.value import SamplingConfig
from autd3_modulation import ModulationBuffer


def main() -> None:
    # ANCHOR: api
    length = 10
    buffer = ModulationBuffer(length)
    buffer[0] = 0xFF

    Modulation(SamplingConfig.FREQ_4K, buffer)
    # ANCHOR_END: api


if __name__ == "__main__":
    main()
