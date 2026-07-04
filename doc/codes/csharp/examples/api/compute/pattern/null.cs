using System.Numerics;
using AUTD3;

namespace DocSamples.ApiComputePatternNull;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        var dst = geometry.PatternBuffer();
        // ANCHOR: api
        Pattern.Null(dst);
        // ANCHOR_END: api
    }
}
