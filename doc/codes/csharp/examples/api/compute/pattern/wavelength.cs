using AUTD3;
using static AUTD3.Units;

namespace AUTD3.DocSamples.ApiComputePatternWavelength;

internal static class Sample
{
    internal static void Run()
    {
        // ANCHOR: api
        var wavelength = Pattern.Wavelength(340.0f * m / s);
        // ANCHOR_END: api
        _ = wavelength;
    }
}
