using System.Numerics;
using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiComputePatternHermiteGaussianExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });

var phases = geometry.PhaseBuffer();
var intensities = geometry.IntensityBuffer();

var target = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
var option = new HermiteGaussianOption(m: 1, n: 1, waist: 10.0f * mm);
var wavelength = Pattern.Wavelength(340.0f * m / s);
Pattern.HermiteGaussianPhase(geometry, target, Vector3.UnitZ, Vector3.UnitX, option, wavelength, phases);
Pattern.HermiteGaussianIntensity(geometry, target, Vector3.UnitZ, Vector3.UnitX, option, wavelength, intensities);
        // HIDE
    }
}
// HIDE_END
