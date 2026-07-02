using System.Numerics;
using AUTD3;
using static AUTD3.Units;

// HIDE
namespace AUTD3.DocSamples.ApiComputePatternPlaneExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

var @out = geometry.PatternBuffer();

Pattern.Plane(
    geometry,
    Vector3.UnitZ,
    Pattern.Wavelength(340.0f * m / s),
    new PlaneOption(
        intensity: Intensity.Max,
        phaseOffset: Phase.Zero
    ),
    @out
);
        // HIDE
    }
}
// HIDE_END
