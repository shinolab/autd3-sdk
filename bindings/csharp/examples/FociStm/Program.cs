// A focus moving along a 30 mm circle at 1 Hz using FociStm.
// Run with: cargo xtask cs example FociStm

using System;
using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

internal static class Program
{
    private static async Task Main()
    {
        using var logGuard = Tracing.Init(new TracingOption());

        using var geometry = new Geometry(new List<Autd3> { new Autd3(Vector3.Zero) });

        using var client = Client.Open(geometry, new TransportOption());
        await client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));

        Console.WriteLine($"devices: {client.NumDevices}");

        var center = geometry.Center + new Vector3(0f, 0f, 150f);
        var points = Enumerable.Range(0, 200)
            .Select(i => 2f * MathF.PI * i / 200)
            .Select(theta => new ControlPoints(new[] { new ControlPoint(center + 30f * new Vector3(MathF.Cos(theta), MathF.Sin(theta), 0f), Phase.Zero) }, Intensity.Max))
            .ToList();

        await client.SendAsync(new SetSilencer());
        await client.SendAsync(new FociStm(new SamplingConfig(1 * Hz * points.Count), points.ToArray(), new FociStmOption()));

        Console.WriteLine("sweeping a focus around a 30 mm circle at 1 Hz — press Ctrl+C to stop");

        var stop = new TaskCompletionSource();
        Console.CancelKeyPress += (_, e) =>
        {
            e.Cancel = true;
            stop.TrySetResult();
        };
        await stop.Task;

        using var zeroPhases = geometry.PhaseBuffer();
        await client.SendAsync(new Pattern(zeroPhases, Intensity.Min));
    }
}
