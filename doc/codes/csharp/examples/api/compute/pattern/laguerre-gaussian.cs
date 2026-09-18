using System.Numerics;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiComputePatternLaguerreGaussian;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        var target = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var axis = Vector3.UnitZ;
        var wavelength = Pattern.Wavelength(340.0f * m / s);
        var dst = geometry.PatternBuffer();

        // ANCHOR: api
        var option = new LaguerreGaussianOption(p: 1, l: 1, waist: 10.0f * mm);
        Pattern.LaguerreGaussianPhase(geometry, target, axis, option, wavelength, dst);
        Pattern.LaguerreGaussianIntensity(geometry, target, axis, option, wavelength, dst);
        // ANCHOR_END: api
    }
}
