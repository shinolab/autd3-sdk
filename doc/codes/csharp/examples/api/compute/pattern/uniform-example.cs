using System.Numerics;
using AUTD3;

// HIDE
namespace DocSamples.ApiComputePatternUniformExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

var dst = geometry.PatternBuffer();

Pattern.Uniform(
    new Emission(
        Phase.Zero,
        Intensity.Max
    ),
    dst
);
        // HIDE
    }
}
// HIDE_END
