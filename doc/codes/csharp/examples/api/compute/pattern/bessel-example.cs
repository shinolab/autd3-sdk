using System.Numerics;
using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiComputePatternBesselExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

var phases = geometry.PhaseBuffer();

Pattern.Bessel(
    geometry,
    geometry.Center + new Vector3(0.0f, 0.0f, 150.0f),
    Vector3.UnitZ,
    18.0f * deg,
    Pattern.Wavelength(340.0f * m / s),
    phases
);
        // HIDE
    }
}
// HIDE_END
