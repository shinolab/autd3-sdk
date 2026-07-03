using System.Numerics;
using AUTD3;
using static AUTD3.Units;
using static AUTD3.HoloUnits;

// HIDE
namespace AUTD3.DocSamples.ApiComputeHoloGreedyExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

var dst = geometry.PatternBuffer();

Holo.Greedy(
    geometry,
    new[]
    {
        new HoloControlPoint(geometry.Center + new Vector3(-30.0f, 0.0f, 150.0f), 2.5e3f * Pa),
        new HoloControlPoint(geometry.Center + new Vector3(30.0f, 0.0f, 150.0f), 2.5e3f * Pa),
    },
    Pattern.Wavelength(340.0f * m / s),
    new GreedyOption(
        phaseQuantizationLevels: 16,
        constraint: EmissionConstraint.Uniform(Intensity.Max),
        directivity: Directivity.Sphere,
        mask: TransducerMask.AllEnabled
    ),
    dst
);
        // HIDE
    }
}
// HIDE_END
