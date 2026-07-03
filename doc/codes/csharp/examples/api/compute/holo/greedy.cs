using System.Numerics;
using AUTD3;
using static AUTD3.Units;
using static AUTD3.HoloUnits;

namespace AUTD3.DocSamples.ApiComputeHoloGreedy;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

        var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var foci = new[]
        {
            new Holo.ControlPoint(center + new Vector3(-30.0f, 0.0f, 0.0f), 2.5e3f * Pa),
            new Holo.ControlPoint(center + new Vector3(30.0f, 0.0f, 0.0f), 2.5e3f * Pa),
        };

        var wavelength = Pattern.Wavelength(340.0f * m / s);
        var option =
            // ANCHOR: option
            new GreedyOption(
                phaseQuantizationLevels: 16,
                constraint: EmissionConstraint.Uniform(Intensity.Max),
                directivity: Directivity.Sphere,
                mask: TransducerMask.AllEnabled
            )
            // ANCHOR_END: option
            ;
        var dst = geometry.PatternBuffer();
        // ANCHOR: api
        Holo.Greedy(geometry, foci, wavelength, option, dst);
        // ANCHOR_END: api
    }
}
