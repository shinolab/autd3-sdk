// Pattern STM: a circle of host-computed focus patterns played back at 1 Hz.
// Run with: cargo xtask cs example PatternStm

using System;
using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

internal static class Program
{
    private const int NumPoints = 200;
    private const float RadiusMm = 30f;

    private static async Task Main()
    {
        using var logGuard = Tracing.Init(new TracingOption());

        using var geometry = new Geometry(new List<Autd3> { new Autd3(Vector3.Zero) });

        await using var client = await Client.OpenAsync(geometry, new TransportOption(), new ClientConfig());

        Console.WriteLine($"devices: {client.NumDevices}");

        var center = geometry.Center + new Vector3(0f, 0f, 150f);
        var wavelength = Pattern.Wavelength(340 * m / s);
        var patterns = Enumerable.Range(0, NumPoints)
            .Select(i =>
            {
                var theta = 2f * MathF.PI * i / NumPoints;
                var target = center + new Vector3(RadiusMm * MathF.Cos(theta), RadiusMm * MathF.Sin(theta), 0f);
                var phases = geometry.PhaseBuffer();
                Pattern.Focus(geometry, target, wavelength, phases);
                return phases;
            })
            .ToArray();

        await client.SendAsync(new SetSilencer());
        await client.SendAsync(new PatternStm(1 * Hz, patterns, Intensity.Max));

        Console.WriteLine("running a 1 Hz circular pattern STM — press Ctrl+C to stop");

        var stop = new TaskCompletionSource();
        Console.CancelKeyPress += (_, e) =>
        {
            e.Cancel = true;
            stop.TrySetResult();
        };
        await stop.Task;
    }
}
