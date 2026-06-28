// Sweeps a focus around a circle two ways: stop-and-wait vs streaming.
// Streaming keeps up to MaxInflight frames in flight via a semaphore.
// Run with: cargo xtask cs example SendModes

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Numerics;
using System.Threading;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;

internal static class Program
{
    private const int TotalPoints = 1000;
    private const int MaxInflight = 127;

    private static void Report(string label, double elapsedSeconds)
    {
        var rate = TotalPoints / elapsedSeconds;
        Console.WriteLine($"{label}: {TotalPoints} updates in {elapsedSeconds:F2}s ({rate:F0} updates/s)");
    }

    private static async Task Configure(Client client, PatternBuffer patterns)
    {
        Pattern.Null(patterns);
        using var builder = client.DatagramBuilder();
        builder
            .Push(new WritePatternBuffer(PatternBank.B0, 0, patterns))
            .Push(new ConfigPattern(PatternBank.B0, 1, 1, PatternDataType.Raw));
        using var datagrams = builder.Build();
        foreach (var frame in datagrams)
        {
            await client.SendCheckedAsync(frame);
        }
    }

    private static Datagrams WriteFocus(Client client, PatternBuffer patterns)
    {
        using var builder = client.DatagramBuilder();
        builder.Push(new WritePatternBuffer(PatternBank.B0, 0, patterns));
        return builder.Build();
    }

    private static async Task Main()
    {
        using var geometry = new Geometry(new List<Device> { new Device(Vector3.Zero) });
        using var client = await Client.OpenAsync(geometry, EtherCrabLink.Create(), new ClientConfig());

        var center = geometry.Center;
        const float radius = 30f;
        var wavelength = Pattern.Wavelength(340f * 1000f);

        using var patterns = geometry.PatternBuffer();
        await Configure(client, patterns);

        var datagrams = new List<Datagrams>(TotalPoints);
        for (var i = 0; i < TotalPoints; i++)
        {
            var theta = 2.0 * Math.PI * i / TotalPoints;
            var target = center + new Vector3(radius * (float)Math.Cos(theta), radius * (float)Math.Sin(theta), 150f);
            Pattern.Focus(geometry, target, wavelength, Intensity.Max, patterns);
            datagrams.Add(WriteFocus(client, patterns));
        }

        Console.WriteLine($"sweeping a focus through {TotalPoints} positions, twice");

        // stop-and-wait: confirm each frame lands before issuing the next.
        var sw = Stopwatch.StartNew();
        foreach (var dg in datagrams)
        {
            foreach (var frame in dg)
            {
                await client.SendCheckedAsync(frame);
            }
        }
        Report("stop-and-wait", sw.Elapsed.TotalSeconds);

        // streaming: keep MaxInflight frames on the wire concurrently.
        using var sem = new SemaphoreSlim(MaxInflight);
        async Task Send(Frame frame)
        {
            await sem.WaitAsync();
            try
            {
                await client.SendCheckedAsync(frame);
            }
            finally
            {
                sem.Release();
            }
        }

        sw.Restart();
        var tasks = datagrams.SelectMany(dg => dg).Select(Send).ToList();
        await Task.WhenAll(tasks);
        Report("streaming", sw.Elapsed.TotalSeconds);

        await client.StopAsync();
        await client.CloseAsync();

        foreach (var dg in datagrams)
        {
            dg.Dispose();
        }
    }
}
