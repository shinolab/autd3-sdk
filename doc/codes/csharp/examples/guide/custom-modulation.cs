using AUTD3;

namespace DocSamples.GuideCustomModulation;

internal static class Sample
{
    internal static void Run()
    {
        // ANCHOR: api
        var length = 10;
        var data = new ModulationBuffer(length);
        data[0] = 0xFF;

        new Modulation(SamplingConfig.Freq4k, data);
        // ANCHOR_END: api
    }
}
