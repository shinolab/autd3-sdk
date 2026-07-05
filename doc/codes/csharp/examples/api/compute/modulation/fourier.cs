using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiComputeModulationFourier;

internal static class Sample
{
    internal static void Run()
    {
        float? scaleFactor = null;
        var clamp = false;
        byte offset = 0x00;
        var option =
            // ANCHOR: option
            new FourierOption(
                scaleFactor,
                clamp,
                offset
            )
            // ANCHOR_END: option
            ;
        var dst = Modulation.ModulationBuffer();

        // Shown standalone in the SineComponent section of the docs.
        // ANCHOR: components
        new SineComponent(
            100 * Hz,
            new SineOption()
        );
        // ANCHOR_END: components

        var components = new[]
        {
            new SineComponent(100 * Hz, new SineOption()),
        };
        // ANCHOR: api
        Modulation.Fourier(components, option, dst);
        // ANCHOR_END: api
    }
}
