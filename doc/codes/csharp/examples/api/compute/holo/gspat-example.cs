using System.Numerics;
using AUTD3;
using static AUTD3.Units;
using static AUTD3.HoloUnits;

// HIDE
namespace AUTD3.DocSamples.ApiComputeHoloGspatExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

var dst = geometry.PatternBuffer();

Holo.Gspat(
    geometry,
    new[]
    {
        new Holo.ControlPoint(geometry.Center + new Vector3(-30.0f, 0.0f, 150.0f), 2.5e3f * Pa),
        new Holo.ControlPoint(geometry.Center + new Vector3(30.0f, 0.0f, 150.0f), 2.5e3f * Pa),
    },
    Pattern.Wavelength(340.0f * m / s),
    new GspatOption(
        repeat: 100,
        constraint: EmissionConstraint.Clamp(Intensity.Min, Intensity.Max),
        directivity: Directivity.Sphere,
        mask: TransducerMask.AllEnabled
    ),
    dst
);
        // HIDE
    }
}
// HIDE_END
