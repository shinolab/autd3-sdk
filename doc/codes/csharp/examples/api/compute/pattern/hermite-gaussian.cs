using System.Numerics;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiComputePatternHermiteGaussian;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        var target = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
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
