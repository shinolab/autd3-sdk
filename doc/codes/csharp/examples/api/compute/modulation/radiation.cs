using AUTD3;
using static AUTD3.Units;

namespace AUTD3.DocSamples.ApiComputeModulationRadiation;

internal static class Sample
{
    internal static void Run()
    {
        var src = Modulation.ModulationBuffer();
        Modulation.Sine(150 * Hz, new SineOption(), src);

        var @out = Modulation.ModulationBuffer();
        // ANCHOR: api
        Modulation.RadiationPressure(src, @out);
        // ANCHOR_END: api
    }
}
