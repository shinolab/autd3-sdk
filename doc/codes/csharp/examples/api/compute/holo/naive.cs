using System.Numerics;
using AUTD3;
using AUTD3.Holo;
using ControlPoint = AUTD3.Holo.ControlPoint;
using static AUTD3.Units;
using static AUTD3.Holo.HoloUnits;

namespace DocSamples.ApiComputeHoloNaive;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var foci = new[]
        {
            new ControlPoint(center + new Vector3(-30.0f, 0.0f, 0.0f), 2.5e3f * Pa),
            new ControlPoint(center + new Vector3(30.0f, 0.0f, 0.0f), 2.5e3f * Pa),
        };

        var wavelength = Pattern.Wavelength(340.0f * m / s);
        var option =
            // ANCHOR: option
            new NaiveOption(
                constraint: EmissionConstraint.Clamp(Intensity.Min, Intensity.Max),
                directivity: Directivity.Sphere,
                backend: new NalgebraBackend(),
                mask: TransducerMask.AllEnabled
            )
            // ANCHOR_END: option
            ;
        var dst = geometry.PatternBuffer();
        // ANCHOR: api
        Holo.Naive(geometry, foci, wavelength, option, dst);
        // ANCHOR_END: api
    }
}
