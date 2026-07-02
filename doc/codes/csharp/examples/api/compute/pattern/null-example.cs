using System.Numerics;
using AUTD3;

// HIDE
namespace AUTD3.DocSamples.ApiComputePatternNullExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

var @out = geometry.PatternBuffer();

Pattern.Null(@out);
        // HIDE
    }
}
// HIDE_END
