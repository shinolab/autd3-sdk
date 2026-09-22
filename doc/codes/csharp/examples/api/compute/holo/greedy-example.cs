using System.Numerics;
using AUTD3;
using AUTD3.Holo;
using static AUTD3.Units;
using static AUTD3.Holo.HoloUnits;

// HIDE
namespace DocSamples.ApiComputeHoloGreedyExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

var phases = geometry.PhaseBuffer();
var intensities = geometry.IntensityBuffer();

Holo.Greedy(
    geometry,
    new[]
    {
        new AmplitudeTarget(geometry.Center + new Vector3(-30.0f, 0.0f, 150.0f), 2.5e3f * Pa),
        new AmplitudeTarget(geometry.Center + new Vector3(30.0f, 0.0f, 150.0f), 2.5e3f * Pa),
    },
    Pattern.Wavelength(340.0f * m / s),
    new GreedyOption
    {
        PhaseQuantizationLevels = 16,
        Constraint = IntensityConstraint.Uniform(Intensity.Max),
        Directivity = Directivity.Sphere,
        Mask = TransducerMask.AllEnabled,
    },
    phases,
    intensities
);
        // HIDE
    }
}
// HIDE_END
