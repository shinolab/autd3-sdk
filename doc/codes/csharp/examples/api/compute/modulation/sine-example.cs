using AUTD3;
using static AUTD3.Units;

// HIDE
namespace AUTD3.DocSamples.ApiComputeModulationSineExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var @out = Modulation.ModulationBuffer();

Modulation.Sine(
    150 * Hz,
    new SineOption(
        intensity: 0xFF,
        offset: 0x80,
        phase: 0.0f * rad,
        clamp: false,
        samplingConfig: SamplingConfig.Freq4k
    ),
    @out
);
        // HIDE
    }
}
// HIDE_END
