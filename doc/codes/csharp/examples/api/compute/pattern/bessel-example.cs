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
var geometry = new Geometry(new[] { new Autd3() });

var phases = geometry.PhaseBuffer();

Pattern.Bessel(
    geometry,
    geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm),
    Vector3.UnitZ,
    18.0f * deg,
    Pattern.Wavelength(340.0f * m / s),
    phases
);
        // HIDE
    }
}
// HIDE_END
