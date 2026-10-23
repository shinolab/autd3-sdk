using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiComputeModulationRadiationExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var buffer = Modulation.ModulationBuffer();
Modulation.Sine(150 * Hz, new SineOption(), SamplingConfig.Freq4k, buffer);

Modulation.RadiationPressureInplace(buffer);
        // HIDE
    }
}
// HIDE_END
