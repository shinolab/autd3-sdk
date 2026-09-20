using System.Numerics;
using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiComputePatternFocusExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

var phases = geometry.PhaseBuffer();

Pattern.Focus(
    geometry,
    geometry.Center + new Vector3(0.0f, 0.0f, 150.0f),
    Pattern.Wavelength(340.0f * m / s),
    phases
);
        // HIDE
    }
}
// HIDE_END
