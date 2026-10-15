using System.Numerics;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiComputePatternLaguerreGaussian;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        var target = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
        var axis = Vector3.UnitZ;
        var wavelength = Pattern.Wavelength(340.0f * m / s);
        var phases = geometry.PhaseBuffer();
        var intensities = geometry.IntensityBuffer();

        // ANCHOR: api
        var option = new LaguerreGaussianOption(p: 1, l: 1, waist: 10.0f * mm);
        Pattern.LaguerreGaussianPhase(geometry, target, axis, option, wavelength, phases);
        Pattern.LaguerreGaussianIntensity(geometry, target, axis, option, wavelength, intensities);
        // ANCHOR_END: api
    }
}
