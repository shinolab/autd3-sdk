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
        var option =
            // ANCHOR: option
            new PlaneOption(
                intensity: Intensity.Max,
                phaseOffset: Phase.Zero
            )
            // ANCHOR_END: option
            ;
        var dst = geometry.PatternBuffer();

        // ANCHOR: api
        Pattern.Plane(geometry, direction, wavelength, option, dst);
        // ANCHOR_END: api
    }
}
