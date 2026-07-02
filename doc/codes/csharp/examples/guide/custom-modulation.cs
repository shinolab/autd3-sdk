using AUTD3;

namespace AUTD3.DocSamples.GuideCustomModulation;

internal static class Sample
{
    internal static void Run()
    {
        // ANCHOR: api
        var length = 10;
        var data = new byte[length];
        data[0] = 0xFF;
        var buffer = ModulationBuffer.FromBytes(data);

        new Modulation(SamplingConfig.Freq4k, buffer);
        // ANCHOR_END: api
    }
}
