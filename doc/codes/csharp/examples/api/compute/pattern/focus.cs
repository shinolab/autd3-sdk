using System.Numerics;
using AUTD3;
using static AUTD3.Units;

namespace AUTD3.DocSamples.ApiComputePatternFocus;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

        var target = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var wavelength = Pattern.Wavelength(340.0f * m / s);
        var option =
            // ANCHOR: option
            new FocusOption(
                intensity: Intensity.Max,
                phaseOffset: Phase.Zero
            )
            // ANCHOR_END: option
            ;
        var dst = geometry.PatternBuffer();

        // ANCHOR: api
        Pattern.Focus(geometry, target, wavelength, option, dst);
        // ANCHOR_END: api
    }
}
