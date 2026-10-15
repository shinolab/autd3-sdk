using AUTD3;
using AUTD3.Holo;
using static AUTD3.Units;
using static AUTD3.Holo.HoloUnits;

namespace DocSamples.ApiComputeHoloNaive;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        var center = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
        var foci = new[]
        {
            new AmplitudeTarget(center + Offset(-30.0f * mm, 0.0f * mm, 0.0f * mm), 2.5e3f * Pa),
            new AmplitudeTarget(center + Offset(30.0f * mm, 0.0f * mm, 0.0f * mm), 2.5e3f * Pa),
        };

        var wavelength = Pattern.Wavelength(340.0f * m / s);
        var constraint = IntensityConstraint.Clamp(Intensity.Min, Intensity.Max);
        var directivity = Directivity.Sphere;
        var mask = TransducerMask.AllEnabled;
        var parallel = true;
        var option =
            // ANCHOR: option
            new NaiveOption
            {
                Constraint = constraint,
                Directivity = directivity,
                Mask = mask,
                Parallel = parallel,
            }
            // ANCHOR_END: option
            ;
        var phases = geometry.PhaseBuffer();
        var intensities = geometry.IntensityBuffer();
        // ANCHOR: api
        Holo.Naive(geometry, foci, wavelength, option, phases, intensities);
        // ANCHOR_END: api
    }
}
