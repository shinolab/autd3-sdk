using AUTD3;

namespace DocSamples.ApiComputeModulationConstant;

internal static class Sample
{
    internal static void Run()
    {
        var dst = Modulation.ModulationBuffer();
        byte intensity = 0xFF;
        // ANCHOR: api
        Modulation.Constant(intensity, dst);
        // ANCHOR_END: api
    }
}
