using System.Numerics;
using AUTD3;

namespace AUTD3.DocSamples.ApiComputePatternUniform;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

        var emission = new Emission(Phase.Zero, Intensity.Max);
        var dst = geometry.PatternBuffer();
        // ANCHOR: api
        Pattern.Uniform(emission, dst);
        // ANCHOR_END: api
    }
}
