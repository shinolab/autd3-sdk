using AUTD3;

namespace AUTD3.DocSamples.ApiComputeModulationConstant;

internal static class Sample
{
    internal static void Run()
    {
        var @out = Modulation.ModulationBuffer();
        byte intensity = 0xFF;
        // ANCHOR: api
        Modulation.Constant(intensity, @out);
        // ANCHOR_END: api
    }
}
