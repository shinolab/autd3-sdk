using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiComputeModulationRadiation;

internal static class Sample
{
    internal static void Run()
    {
        var src = Modulation.ModulationBuffer();
        Modulation.Sine(150 * Hz, new SineOption(), src);

        var dst = Modulation.ModulationBuffer();
        // ANCHOR: api
        Modulation.RadiationPressure(src, dst);
        // ANCHOR_END: api
    }
}
