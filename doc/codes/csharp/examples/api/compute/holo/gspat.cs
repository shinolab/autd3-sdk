using System.Numerics;
using AUTD3;
using static AUTD3.Units;
using static AUTD3.HoloUnits;

namespace AUTD3.DocSamples.ApiComputeHoloGspat;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

        var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var foci = new[]
        {
            new HoloControlPoint(center + new Vector3(-30.0f, 0.0f, 0.0f), 2.5e3f * Pa),
            new HoloControlPoint(center + new Vector3(30.0f, 0.0f, 0.0f), 2.5e3f * Pa),
        };

        var wavelength = Pattern.Wavelength(340.0f * m / s);
        var option =
            // ANCHOR: option
            new GspatOption(
                repeat: 100,
                constraint: EmissionConstraint.Clamp(Intensity.Min, Intensity.Max),
                directivity: Directivity.Sphere,
                mask: TransducerMask.AllEnabled
            )
            // ANCHOR_END: option
            ;
        var @out = geometry.PatternBuffer();
        // ANCHOR: api
        Holo.Gspat(geometry, foci, wavelength, option, @out);
        // ANCHOR_END: api
    }
}
