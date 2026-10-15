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
var geometry = new Geometry(new[] { new Autd3() });

var phases = geometry.PhaseBuffer();

Holo.Greedy(
    geometry,
    new[]
    {
        new AmplitudeTarget(geometry.Center + Offset(-30.0f * mm, 0.0f * mm, 150.0f * mm), 2.5e3f * Pa),
        new AmplitudeTarget(geometry.Center + Offset(30.0f * mm, 0.0f * mm, 150.0f * mm), 2.5e3f * Pa),
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
