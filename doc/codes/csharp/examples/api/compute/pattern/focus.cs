using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiComputePatternFocus;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        var target = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
        var wavelength = Pattern.Wavelength(340.0f * m / s);
        var phases = geometry.PhaseBuffer();

        // ANCHOR: api
        Pattern.Focus(geometry, target, wavelength, phases);
        // ANCHOR_END: api
    }
}
