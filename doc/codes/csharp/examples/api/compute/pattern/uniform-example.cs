using System.Numerics;
using AUTD3;

// HIDE
namespace AUTD3.DocSamples.ApiComputePatternUniformExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

var @out = geometry.PatternBuffer();

Pattern.Uniform(
    new Emission(Phase.Zero, Intensity.Max),
    @out
);
        // HIDE
    }
}
// HIDE_END
