using System.Numerics;
using AUTD3;
using AUTD3.Holo;
using ControlPoint = AUTD3.Holo.ControlPoint;
using static AUTD3.Units;
using static AUTD3.Holo.HoloUnits;

// HIDE
namespace DocSamples.ApiComputeHoloNaiveExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

var dst = geometry.PatternBuffer();

Holo.Naive(
    geometry,
    new[]
    {
        new ControlPoint(geometry.Center + new Vector3(-30.0f, 0.0f, 150.0f), 2.5e3f * Pa),
        new ControlPoint(geometry.Center + new Vector3(30.0f, 0.0f, 150.0f), 2.5e3f * Pa),
    },
    Pattern.Wavelength(340.0f * m / s),
    new NaiveOption(
        constraint: EmissionConstraint.Clamp(Intensity.Min, Intensity.Max),
        directivity: Directivity.Sphere,
        backend: new NalgebraBackend(),
        mask: TransducerMask.AllEnabled
    ),
    dst
);
        // HIDE
    }
}
// HIDE_END
