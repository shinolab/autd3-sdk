using System.Numerics;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiComputePatternHermiteGaussian;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        var target = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
        var axis = Vector3.UnitZ;
        var xDir = Vector3.UnitX;
        var wavelength = Pattern.Wavelength(340.0f * m / s);
        var phases = geometry.PhaseBuffer();
        var intensities = geometry.IntensityBuffer();

        // ANCHOR: api
        var option = new HermiteGaussianOption(m: 1, n: 0, waist: 10.0f * mm);
        Pattern.HermiteGaussianPhase(geometry, target, axis, xDir, option, wavelength, phases);
        Pattern.HermiteGaussianIntensity(geometry, target, axis, xDir, option, wavelength, intensities);
        // ANCHOR_END: api
    }
}
