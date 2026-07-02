using AUTD3;
using static AUTD3.Units;

// HIDE
namespace AUTD3.DocSamples.ApiComputeModulationSquareExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var @out = Modulation.ModulationBuffer();

Modulation.Square(
    150 * Hz,
    new SquareOption(
        low: byte.MinValue,
        high: byte.MaxValue,
        duty: 0.5f,
        samplingConfig: SamplingConfig.Freq4k
    ),
    @out
);
        // HIDE
    }
}
// HIDE_END
