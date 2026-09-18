using System.Numerics;
using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiComputePatternLaguerreGaussianExample;

internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

var dst = geometry.PatternBuffer();

var target = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
var option = new LaguerreGaussianOption(p: 0, l: 1, waist: 10.0f * mm);
var wavelength = Pattern.Wavelength(340.0f * m / s);
Pattern.LaguerreGaussianPhase(geometry, target, Vector3.UnitZ, option, wavelength, dst);
Pattern.LaguerreGaussianIntensity(geometry, target, Vector3.UnitZ, option, wavelength, dst);
        // HIDE
    }
}
// HIDE_END
