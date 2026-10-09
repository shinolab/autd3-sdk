using AUTD3;

namespace DocSamples.ApiComputeModulationConstant;

internal static class Sample
{
    internal static void Run()
    {
        var dst = Modulation.ModulationBuffer();
        byte amplitude = 0xFF;
        // ANCHOR: api
        Modulation.Constant(amplitude, dst);
        // ANCHOR_END: api
    }
}
