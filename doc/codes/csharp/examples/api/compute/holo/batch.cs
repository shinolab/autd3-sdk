using System.Linq;
using AUTD3;
using AUTD3.Holo;
using static AUTD3.Units;
using static AUTD3.Holo.HoloUnits;

namespace DocSamples.ApiComputeHoloBatch;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });
        var center = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
        var wavelength = Pattern.Wavelength(340.0f * m / s);
        var option = new GsOption();

        // ANCHOR: api
        var problems = 64;
        var foci = Enumerable.Range(0, problems)
            .Select(i => new AmplitudeTarget(center + Offset(i * 0.5f * mm, 0.0f * mm, 0.0f * mm), 5e3f * Pa))
            .ToArray();

        var phases = Enumerable.Range(0, problems).Select(_ => geometry.PhaseBuffer()).ToArray();
        var intensities = Enumerable.Range(0, problems).Select(_ => geometry.IntensityBuffer()).ToArray();
        Holo.GsBatch(geometry, foci, wavelength, option, phases, intensities);
        // ANCHOR_END: api
    }
}
