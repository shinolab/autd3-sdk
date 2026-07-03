using AUTD3;

// HIDE
namespace AUTD3.DocSamples.ApiComputeModulationConstantExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var dst = Modulation.ModulationBuffer();

Modulation.Constant(0xFF, dst);
        // HIDE
    }
}
// HIDE_END
