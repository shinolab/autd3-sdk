using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiComputePatternFocusExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });

var phases = geometry.PhaseBuffer();

Pattern.Focus(
    geometry,
    geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm),
    Pattern.Wavelength(340.0f * m / s),
    phases
);
        // HIDE
    }
}
// HIDE_END
