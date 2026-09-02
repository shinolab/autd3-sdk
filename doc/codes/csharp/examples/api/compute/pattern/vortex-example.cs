using System.Numerics;
using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiComputePatternVortexExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

var dst = geometry.PatternBuffer();

Pattern.Vortex(
    geometry,
    geometry.Center + new Vector3(0.0f, 0.0f, 150.0f),
    Vector3.UnitZ,
    1,
    Pattern.Wavelength(340.0f * m / s),
    new VortexOption(
        intensity: Intensity.Max,
        phaseOffset: Phase.Zero
    ),
    dst
);
        // HIDE
    }
}
// HIDE_END
