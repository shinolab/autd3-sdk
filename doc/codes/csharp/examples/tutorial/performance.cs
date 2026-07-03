using System;
using System.Collections.Generic;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using static AUTD3.Units;

namespace AUTD3.DocSamples.TutorialPerformance;

internal static class Sample
{
    private const int NumPoints = 1000;
    private const float RadiusMm = 30.0f;

    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

        var client = await Client.OpenAsync(geometry, EtherCrabLink.Create(), new ClientConfig());

        var patterns = geometry.PatternBuffer();

        // ANCHOR: configure
        var builder = client.DatagramBuilder();
        builder.Push(SetSilencer.Disable());
        builder.Push(new WritePatternBuffer(
            PatternBank.B0,
            0,
            patterns
        ));
        builder.Push(new ConfigPattern(
            PatternBank.B0,
            SamplingConfig.Freq40k,
            1,
            PatternDataType.Raw,
            LoopBehavior.Infinite
        ));
        foreach (var frame in builder.Build())
        {
            await client.SendCheckedAsync(frame);
        }
        // ANCHOR_END: configure

        var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var wavelength = Pattern.Wavelength(340.0f * m / s);

        // ANCHOR: hot_loop
        var pending = new Queue<ResponseToken>();
        for (var i = 0; i < NumPoints; i++)
        {
            var theta = 2.0f * MathF.PI * i / NumPoints;
            var target = center + new Vector3(RadiusMm * MathF.Cos(theta), RadiusMm * MathF.Sin(theta), 0.0f);
            Pattern.Focus(
                geometry,
                target,
                wavelength,
                new FocusOption(),
                patterns
            );

            var hotBuilder = client.DatagramBuilder();
            hotBuilder.Push(new WritePatternBuffer(
                PatternBank.B0,
                0,
                patterns
            ));
            foreach (var frame in hotBuilder.Build())
            {
                if (pending.Count >= Client.MaxInflight)
                {
                    await pending.Dequeue().AwaitAsync();
                }
                pending.Enqueue(await client.SendAsync(frame));
            }
        }
        while (pending.Count > 0)
        {
            await pending.Dequeue().AwaitAsync();
        }
        // ANCHOR_END: hot_loop

        await client.StopAsync();
        await client.CloseAsync();
    }
}
