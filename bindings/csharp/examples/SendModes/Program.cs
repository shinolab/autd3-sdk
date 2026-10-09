// Sweeps a focus around a circle two ways: stop-and-wait vs streaming.
// Run with: cargo xtask cs example SendModes

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

internal static class Program
{
    private const int TotalPoints = 1000;

    private static void Report(string label, double elapsedSeconds)
    {
        var rate = TotalPoints / elapsedSeconds;
        Console.WriteLine($"{label}: {TotalPoints} updates in {elapsedSeconds:F2}s ({rate:F0} updates/s)");
    }

    private static async Task Configure(Client client, PhaseBuffer phases)
    {
        await client.SendAsync(Command.Sequence(
            new WritePatternBuffer(PatternBank.B0, 0, phases, Intensity.Min),
            new ConfigPattern(PatternBank.B0, SamplingConfig.Freq4k, 1)));
    }

    private static WritePatternBuffer WriteFocus(PhaseBuffer phases) =>
        new WritePatternBuffer(PatternBank.B0, 0, phases, Intensity.Max);

    private static async Task Main()
    {
        using var logGuard = Tracing.Init(new TracingOption());

        using var geometry = new Geometry(new List<Autd3> { new Autd3(Vector3.Zero) });
        await using var client = await Client.OpenAsync(geometry, new TransportOption(), new ClientConfig());

        var center = geometry.Center;
        const float radius = 30f;
        var wavelength = Pattern.Wavelength(340 * m / s);

        using var phases = geometry.PhaseBuffer();
        await Configure(client, phases);

        var targets = new List<Vector3>(TotalPoints);
        for (var i = 0; i < TotalPoints; i++)
        {
            var theta = 2.0 * Math.PI * i / TotalPoints;
            targets.Add(center + new Vector3(radius * (float)Math.Cos(theta), radius * (float)Math.Sin(theta), 150f));
        }

        Console.WriteLine($"sweeping a focus through {TotalPoints} positions, twice");

        // stop-and-wait: confirm each frame lands before issuing the next.
        var sw = Stopwatch.StartNew();
        foreach (var target in targets)
        {
            Pattern.Focus(geometry, target, wavelength, phases);
            await client.SendAsync(WriteFocus(phases));
        }
        Report("stop-and-wait", sw.Elapsed.TotalSeconds);

        // streaming: keep Client.MaxInflight frames on the wire, draining the oldest response once the window is full.
        sw.Restart();
        var pending = new Queue<ResponseFuture>();
        using var frames = new Frames();
        foreach (var target in targets)
        {
            Pattern.Focus(geometry, target, wavelength, phases);
            frames.EncodeInto(geometry, WriteFocus(phases));
            foreach (var frame in frames)
            {
                if (pending.Count >= Client.MaxInflight)
                {
                    (await pending.Dequeue()).Check();
                }
                pending.Enqueue(await client.SendFrameAsync(frame));
            }
        }
        while (pending.Count > 0)
        {
            (await pending.Dequeue()).Check();
        }
        Report("streaming", sw.Elapsed.TotalSeconds);

        await client.SilentStopAsync();
    }
}
