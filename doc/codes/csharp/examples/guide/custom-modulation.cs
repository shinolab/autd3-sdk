using AUTD3;

namespace DocSamples.GuideCustomModulation;

internal static class Sample
{
    internal static void Run()
    {
        // ANCHOR: api
        var length = 10;
        var buffer = new ModulationBuffer(length);
        buffer[0] = 0xFF;

        new Modulation(SamplingConfig.Freq4k, buffer);
        // ANCHOR_END: api
    }
}
