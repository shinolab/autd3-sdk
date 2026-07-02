using AUTD3;
using static AUTD3.Units;

namespace AUTD3.DocSamples.ApiComputeModulationSquare;

internal static class Sample
{
    internal static void Run()
    {
        var freq = 150 * Hz;
        var option =
            // ANCHOR: option
            new SquareOption(
                low: byte.MinValue,
                high: byte.MaxValue,
                duty: 0.5f,
                samplingConfig: SamplingConfig.Freq4k
            )
            // ANCHOR_END: option
            ;
        var @out = Modulation.ModulationBuffer();
        // ANCHOR: api
        Modulation.Square(freq, option, @out);
        // ANCHOR_END: api
    }
}
