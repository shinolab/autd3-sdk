using System;
using System.Numerics;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.GuideCustomPattern;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        var target = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
        var wavelength = Pattern.Wavelength(340.0f * m / s);

        // ANCHOR: api
        var phases = geometry.PhaseBuffer();
        foreach (var device in geometry)
        {
            var slot = phases[device.Idx];
            for (var t = 0; t < device.NumTransducers; t++)
            {
                var dist = Vector3.Distance(target, device.Position(t));
                slot[t] = (Phase)(-dist / wavelength.Mm * 2.0f * MathF.PI * rad);
            }
        }

        new Pattern(phases, Intensity.Max);
        // ANCHOR_END: api
    }
}
