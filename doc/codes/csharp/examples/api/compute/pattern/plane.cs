using System.Numerics;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiComputePatternPlane;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        var direction = Vector3.UnitZ;
        var wavelength = Pattern.Wavelength(340.0f * m / s);
        var phases = geometry.PhaseBuffer();

        // ANCHOR: api
        Pattern.Plane(geometry, direction, wavelength, phases);
        // ANCHOR_END: api
    }
}
