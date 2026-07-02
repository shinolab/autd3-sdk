using AUTD3;
using static AUTD3.Units;

// HIDE
namespace AUTD3.DocSamples.ApiComputeModulationRadiationExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var src = Modulation.ModulationBuffer();
Modulation.Sine(150 * Hz, new SineOption(), src);

var @out = Modulation.ModulationBuffer();

Modulation.RadiationPressure(src, @out);
        // HIDE
    }
}
// HIDE_END
