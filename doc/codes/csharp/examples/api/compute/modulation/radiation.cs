using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiComputeModulationRadiation;

internal static class Sample
{
    internal static void Run()
    {
        var buffer = Modulation.ModulationBuffer();
        Modulation.Sine(150 * Hz, new SineOption(), SamplingConfig.Freq4k, buffer);

        // ANCHOR: api
        Modulation.RadiationPressureInplace(buffer);
        // ANCHOR_END: api
    }
}
