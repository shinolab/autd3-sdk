using AUTD3;
using static AUTD3.Units;

namespace AUTD3.DocSamples.ApiComputeModulationFourier;

internal static class Sample
{
    internal static void Run()
    {
        var option =
            // ANCHOR: option
            new FourierOption(
                scaleFactor: null,
                clamp: false,
                offset: 0x00
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
