using System;
using System.Linq;
using AUTD3;
using AUTD3.Holo;
using static AUTD3.Units;
using static AUTD3.Holo.HoloUnits;

namespace DocSamples.ApiComputeHoloGreedy;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        var center = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
        var foci = new[]
        {
            new AmplitudeTarget(center + Offset(-30.0f * mm, 0.0f * mm, 0.0f * mm), 2.5e3f * Pa),
            new AmplitudeTarget(center + Offset(30.0f * mm, 0.0f * mm, 0.0f * mm), 2.5e3f * Pa),
        };

        var wavelength = Pattern.Wavelength(340.0f * m / s);
        byte phaseQuantizationLevels = 16;
        var directivity = Directivity.Sphere;
        var mask = TransducerMask.AllEnabled;
        var option =
            // ANCHOR: option
            new GreedyOption
            {
                PhaseQuantizationLevels = phaseQuantizationLevels,
                Directivity = directivity,
                Mask = mask,
            }
            // ANCHOR_END: option
            ;
        var intensities = geometry.IntensityBuffer();
        var order = Enumerable.Range(0, geometry.NumTransducers).Select(i => (uint)i).ToArray();
        Random.Shared.Shuffle(order);
        var dst = geometry.PhaseBuffer();
        // ANCHOR: api
        Holo.Greedy(geometry, foci, wavelength, intensities, order, option, dst);
        // ANCHOR_END: api
    }
}
