using AUTD3;
using static AUTD3.Units;

namespace AUTD3.DocSamples.ApiComputeModulationSine;

internal static class Sample
{
    internal static void Run()
    {
        var freq = 150 * Hz;
        var option =
            // ANCHOR: option
            new SineOption(
                intensity: 0xFF,
                offset: 0x80,
                phase: 0.0f * rad,
                clamp: false,
                samplingConfig: SamplingConfig.Freq4k
            )
            // ANCHOR_END: option
            ;
        var dst = Modulation.ModulationBuffer();
        // ANCHOR: api
        Modulation.Sine(freq, option, dst);
        // ANCHOR_END: api

        // ANCHOR: nearest
        Modulation.Sine(Nearest(150.5f * Hz), option, dst);
        // ANCHOR_END: nearest
    }
}
