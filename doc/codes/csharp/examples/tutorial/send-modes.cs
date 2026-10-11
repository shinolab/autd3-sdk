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
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        await using var client = await Client.OpenAsync(geometry, new TransportOption(), new ClientConfig());

        await client.SendAsync(new SetSilencer());

        var wavelength = Pattern.Wavelength(340.0f * m / s);

        // ANCHOR: targets
        // Prepare 1000 focus points along a circle 150 mm above the array center.
        var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var targets = new Vector3[NumPoints];
        for (var i = 0; i < NumPoints; i++)
        {
            var theta = 2.0f * MathF.PI * i / NumPoints;
            targets[i] = center + new Vector3(RadiusMm * MathF.Cos(theta), RadiusMm * MathF.Sin(theta), 0.0f);
        }
        // ANCHOR_END: targets

        await StopAndWait(client, geometry, targets, wavelength);
        await Streaming(client, geometry, targets, wavelength);
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
                if (pending.Count >= Client.MaxInflight)
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
