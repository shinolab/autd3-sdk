using System.Numerics;
using AUTD3;

namespace AUTD3.DocSamples.ApiComputePatternNull;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

        var @out = geometry.PatternBuffer();
        // ANCHOR: api
        Pattern.Null(@out);
        // ANCHOR_END: api
    }
}
