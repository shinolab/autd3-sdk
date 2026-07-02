using System.Numerics;
using AUTD3;

namespace AUTD3.DocSamples.ApiComputePatternUniform;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

        var emission = new Emission(Phase.Zero, Intensity.Max);
        var @out = geometry.PatternBuffer();
        // ANCHOR: api
        Pattern.Uniform(emission, @out);
        // ANCHOR_END: api
    }
}
