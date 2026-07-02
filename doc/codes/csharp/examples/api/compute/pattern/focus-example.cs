using System.Numerics;
using AUTD3;
using static AUTD3.Units;

// HIDE
namespace AUTD3.DocSamples.ApiComputePatternFocusExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

var @out = geometry.PatternBuffer();

Pattern.Focus(
    geometry,
    geometry.Center + new Vector3(0.0f, 0.0f, 150.0f),
    Pattern.Wavelength(340.0f * m / s),
    new FocusOption(
        intensity: Intensity.Max,
        phaseOffset: Phase.Zero
    ),
    @out
);
        // HIDE
    }
}
// HIDE_END
