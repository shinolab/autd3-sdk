using System;
using System.Collections.Generic;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.TutorialSendModes;

internal static class Sample
{
    private const int NumPoints = 1000;
    private const float RadiusMm = 30.0f;

    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        using var client = Client.Open(geometry, new TransportOption());
        await client.SendAsync(new Clear());
        await client.SendAsync(new Synchronize());

        await client.SendAsync(new SetSilencer());

        var wavelength = Pattern.Wavelength(340.0f * m / s);

        // ANCHOR: targets
        // Prepare 1000 focus points along a circle 150 mm above the array center.
        var center = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
        var targets = new Vector3[NumPoints];
        for (var i = 0; i < NumPoints; i++)
        {
            var theta = 2.0f * MathF.PI * i / NumPoints;
            targets[i] = center + Offset(RadiusMm * MathF.Cos(theta) * mm, RadiusMm * MathF.Sin(theta) * mm, 0.0f * mm);
        }
        // ANCHOR_END: targets

        await StopAndWait(client, geometry, targets, wavelength);
        await Streaming(client, geometry, targets, wavelength);

        await client.SendAsync(new Pattern(geometry.PhaseBuffer(), Intensity.Min));
    }

    private static async Task StopAndWait(Client client, Geometry geometry, Vector3[] targets, Length wavelength)
    {
        // ANCHOR: stop_and_wait
        var phases = geometry.PhaseBuffer();
        foreach (var target in targets)
        {
            Pattern.Focus(
                geometry,
                target,
                wavelength,
                phases
            );
            await client.SendAsync(new Pattern(phases, Intensity.Max));
        }
        // ANCHOR_END: stop_and_wait
    }

    private static async Task Streaming(Client client, Geometry geometry, Vector3[] targets, Length wavelength)
    {
        // ANCHOR: streaming
        var phases = geometry.PhaseBuffer();
        using var frames = new Frames();
        var pending = new Queue<ResponseFuture>();
        foreach (var target in targets)
        {
            Pattern.Focus(
                geometry,
                target,
                wavelength,
                phases
            );
            frames.EncodeInto(geometry, new Pattern(phases, Intensity.Max));
            foreach (var frame in frames)
            {
                if (pending.Count >= Params.MaxInflight)
                {
                    (await pending.Dequeue()).Check();
                }
                pending.Enqueue(await client.SendFrameAsync(frame));
            }
        }
        // Drain the remaining responses.
        while (pending.Count > 0)
        {
            (await pending.Dequeue()).Check();
        }
        // ANCHOR_END: streaming
    }
}
