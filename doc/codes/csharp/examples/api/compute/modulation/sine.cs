using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiComputeModulationSine;

internal static class Sample
{
    internal static void Run()
    {
        var freq = 150 * Hz;
        byte amplitude = 0xFF;
        byte offset = 0x80;
        var phase = 0.0f * rad;
        var clamp = false;
        var samplingConfig = SamplingConfig.Freq4k;
        var option =
            // ANCHOR: option
            new SineOption(
                amplitude,
                offset,
                phase,
                clamp,
                samplingConfig
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
