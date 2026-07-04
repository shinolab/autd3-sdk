using System;
using System.Numerics;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.GuideCustomPattern;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        var target = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var wavelength = Pattern.Wavelength(340.0f * m / s);

        // ANCHOR: api
        var emissions = geometry.PatternBuffer();
        foreach (var device in geometry)
        {
            var slot = emissions[device.Idx];
            for (var t = 0; t < device.NumTransducers; t++)
            {
                var dist = Vector3.Distance(target, device.Position(t));
                slot[t] = new Emission(
                    (Phase)(-dist / wavelength.Mm * 2.0f * MathF.PI * rad),
                    Intensity.Max);
            }
        }

        new Pattern(emissions);
        // ANCHOR_END: api
    }
}
