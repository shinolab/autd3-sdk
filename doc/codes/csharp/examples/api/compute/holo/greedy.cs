using System.Numerics;
using AUTD3;
using AUTD3.Holo;
using static AUTD3.Units;
using static AUTD3.Holo.HoloUnits;

namespace DocSamples.ApiComputeHoloGreedy;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var foci = new[]
        {
            new AmplitudeTarget(center + new Vector3(-30.0f, 0.0f, 0.0f), 2.5e3f * Pa),
            new AmplitudeTarget(center + new Vector3(30.0f, 0.0f, 0.0f), 2.5e3f * Pa),
        };

        var wavelength = Pattern.Wavelength(340.0f * m / s);
        byte phaseQuantizationLevels = 16;
        var constraint = EmissionConstraint.Uniform(Intensity.Max);
        var directivity = Directivity.Sphere;
        var mask = TransducerMask.AllEnabled;
        var option =
            // ANCHOR: option
            new GreedyOption(
                phaseQuantizationLevels,
                constraint,
                directivity,
                mask
            )
            // ANCHOR_END: option
            ;
        var dst = geometry.PatternBuffer();
        // ANCHOR: api
        Holo.Greedy(geometry, foci, wavelength, option, dst);
        // ANCHOR_END: api
    }
}
