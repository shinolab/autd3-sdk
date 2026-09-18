using System.Numerics;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiComputePatternVortex;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        var target = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var axis = Vector3.UnitZ;
        var order = 1;
        var wavelength = Pattern.Wavelength(340.0f * m / s);
        var dst = geometry.PatternBuffer();

        // ANCHOR: api
        Pattern.Vortex(geometry, target, axis, order, wavelength, dst);
        // ANCHOR_END: api
    }
}
