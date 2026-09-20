using System.Numerics;
using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiComputePatternSetExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

var phases = geometry.PhaseBuffer();
var intensities = geometry.IntensityBuffer();

Pattern.SetIntensity(new Intensity(0x80), intensities);
Pattern.Focus(
    geometry,
    geometry.Center + new Vector3(0.0f, 0.0f, 150.0f),
    Pattern.Wavelength(340.0f * m / s),
    phases
);
Pattern.AddPhase(Phase.Pi, phases);

Pattern.SetIntensity(Intensity.Min, intensities);
        // HIDE
    }
}
// HIDE_END
