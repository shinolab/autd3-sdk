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

Holo.Greedy(
    geometry,
    new[]
    {
        new AmplitudeTarget(geometry.Center + new Vector3(-30.0f, 0.0f, 150.0f), 2.5e3f * Pa),
        new AmplitudeTarget(geometry.Center + new Vector3(30.0f, 0.0f, 150.0f), 2.5e3f * Pa),
    },
    Pattern.Wavelength(340.0f * m / s),
    Intensity.Max,
    new GreedyOption
    {
        PhaseQuantizationLevels = 16,
        Directivity = Directivity.Sphere,
        Mask = TransducerMask.AllEnabled,
    },
    phases
);
        // HIDE
    }
}
// HIDE_END
