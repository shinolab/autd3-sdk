from autd3.commands import Modulation
from autd3.value import SamplingConfig
from autd3_modulation import ModulationBuffer


def main() -> None:
    # ANCHOR: api
    length = 10
    data = ModulationBuffer(length)
    data[0] = 0xFF

    Modulation(SamplingConfig.FREQ_4K, data)
    # ANCHOR_END: api


if __name__ == "__main__":
    main()
