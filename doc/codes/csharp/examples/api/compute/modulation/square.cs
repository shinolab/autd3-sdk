using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiComputeModulationSquare;

internal static class Sample
{
    internal static void Run()
    {
        var freq = 150 * Hz;
        var low = byte.MinValue;
        var high = byte.MaxValue;
        var duty = 0.5f;
        var samplingConfig = SamplingConfig.Freq4k;
        var option =
            // ANCHOR: option
            new SquareOption(
                low,
                high,
                duty,
                samplingConfig
            )
            // ANCHOR_END: option
            ;
        var dst = Modulation.ModulationBuffer();
        // ANCHOR: api
        Modulation.Square(freq, option, dst);
        // ANCHOR_END: api
    }
}
