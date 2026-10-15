using System.Numerics;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiComputePatternBessel;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        var apex = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
        var direction = Vector3.UnitZ;
        var theta = 18.0f * deg;
        var wavelength = Pattern.Wavelength(340.0f * m / s);
        var phases = geometry.PhaseBuffer();

        // ANCHOR: api
        Pattern.Bessel(geometry, apex, direction, theta, wavelength, phases);
        // ANCHOR_END: api
    }
}
