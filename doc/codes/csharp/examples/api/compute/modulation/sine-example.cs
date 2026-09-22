using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiComputeModulationSineExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var dst = Modulation.ModulationBuffer();

Modulation.Sine(
    150 * Hz,
    new SineOption
    {
        Amplitude = 0xFF,
        Offset = 0x80,
        Phase = 0.0f * rad,
        Clamp = false,
        SamplingConfig = SamplingConfig.Freq4k,
    },
    dst
);
        // HIDE
    }
}
// HIDE_END
