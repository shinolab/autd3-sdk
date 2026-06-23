// One-shot (stop-and-wait) command latency in low-latency mode.
// Run with: cargo xtask cs example LowLatency

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;

internal static class Program
{
    private const int Iterations = 1000;
    private const int Warmup = 10;
    private const bool EnableLowLatency = true;

    private static async Task Main()
    {
        using var geometry = new Geometry(new List<Device> { new Device(Vector3.Zero) });
        using var client = await Client.OpenAsync(
            geometry, EtherCrabLink.Create(), new ClientConfig(lowLatency: EnableLowLatency));

        Console.WriteLine($"devices: {client.NumDevices}");

        var target = geometry.Center + new Vector3(0f, 0f, 150f);
        var wavelength = Pattern.Wavelength(340f * 1000f);
        using var patterns = client.PatternBuffer();
        Pattern.Focus(geometry, target, wavelength, Intensity.Min, patterns);
        using var builder = client.DatagramBuilder();
        builder.Push(new Pattern(patterns));
        using var datagrams = builder.Build();

        var frame = datagrams[0];
        for (var i = 0; i < Warmup; i++)
        {
            await client.SendCheckedAsync(frame);
        }

        var latencies = new List<double>(Iterations);
        var sw = new Stopwatch();
        for (var i = 0; i < Iterations; i++)
        {
            sw.Restart();
            await client.SendCheckedAsync(frame);
            sw.Stop();
            latencies.Add(sw.Elapsed.TotalMicroseconds);
        }
        latencies.Sort();

        var sum = 0.0;
        foreach (var l in latencies)
        {
            sum += l;
        }
        var avg = sum / Iterations;

        Console.WriteLine($"one-shot latency over {Iterations} sends (low_latency={EnableLowLatency}):");
        Console.WriteLine(
            $"  min={latencies[0]:F1}us  p50={latencies[Iterations / 2]:F1}us  avg={avg:F1}us"
            + $"  p99={latencies[Iterations * 99 / 100]:F1}us  max={latencies[^1]:F1}us");

        await client.CloseAsync();
    }
}
