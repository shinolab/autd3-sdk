using System;
using System.Numerics;
using AUTD3;
using static AUTD3.Units;

namespace AUTD3.DocSamples.GuideCustomPattern;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

        var target = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var wavelength = Pattern.Wavelength(340.0f * m / s);

        // ANCHOR: api
        var emissions = new Emission[geometry.NumDevices][];
        for (var d = 0; d < geometry.NumDevices; d++)
        {
            var device = geometry[d];
            var slot = new Emission[device.NumTransducers];
            for (var t = 0; t < device.NumTransducers; t++)
            {
                var dist = Vector3.Distance(target, device.Position(t));
                var phase = (byte)(int)Math.Round(-dist / wavelength.Mm * 256.0);
                slot[t] = new Emission(new Phase(phase), Intensity.Max);
            }
            emissions[d] = slot;
        }
        var buffer = PatternBuffer.FromArray(emissions);

        new Pattern(buffer);
        // ANCHOR_END: api
    }
}
