using System;
using System.Collections.Generic;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using static AUTD3.Units;

namespace DocSamples.TutorialSendModes;

internal static class Sample
{
    private const int NumPoints = 1000;
    private const float RadiusMm = 30.0f;

    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        var client = await Client.OpenAsync(geometry, new EchocatLinkOption(), new ClientConfig());

        var builder = client.DatagramBuilder();
        builder.Push(new SetSilencer());
        foreach (var frame in builder.Build())
        {
            await client.SendCheckedAsync(frame);
        }

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

        await client.StopAsync();
        await client.CloseAsync();
    }

    private static async Task StopAndWait(Client client, Geometry geometry, Vector3[] targets, Length wavelength)
    {
        // ANCHOR: stop_and_wait
        var patterns = geometry.PatternBuffer();
        foreach (var target in targets)
        {
            Pattern.Focus(
                geometry,
                target,
                wavelength,
                new FocusOption(),
                patterns
            );
            var builder = client.DatagramBuilder();
            builder.Push(new Pattern(patterns));
            foreach (var frame in builder.Build())
            {
                await client.SendCheckedAsync(frame);
            }
        }
        // ANCHOR_END: stop_and_wait
    }

    private static async Task Streaming(Client client, Geometry geometry, Vector3[] targets, Length wavelength)
    {
        // ANCHOR: streaming
        var patterns = geometry.PatternBuffer();
        var pending = new Queue<ResponseToken>();
        foreach (var target in targets)
        {
            Pattern.Focus(
                geometry,
                target,
                wavelength,
                new FocusOption(),
                patterns
            );
            var builder = client.DatagramBuilder();
            builder.Push(new Pattern(patterns));
            foreach (var frame in builder.Build())
            {
                if (pending.Count >= Client.MaxInflight)
                {
                    (await pending.Dequeue()).Check();
                }
                pending.Enqueue(await client.SendAsync(frame));
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
