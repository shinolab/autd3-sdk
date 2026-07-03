using AUTD3;
using static AUTD3.Units;

// HIDE
namespace AUTD3.DocSamples.ApiComputeModulationFourierExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var dst = Modulation.ModulationBuffer();

Modulation.Fourier(
    new[]
    {
        new SineComponent(
            100 * Hz,
            new SineOption()
        ),
    },
    new FourierOption(
        scaleFactor: null,
        clamp: false,
        offset: 0x00
    ),
    dst
);
        // HIDE
    }
}
// HIDE_END
