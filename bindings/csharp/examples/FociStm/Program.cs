// A focus moving along a 30 mm circle at 1 Hz using FociStm.
// Run with: cargo xtask cs example FociStm

using System;
using System.Collections.Generic;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using static AUTD3.Units;

internal static class Program
{
    private static async Task Main()
    {
        using var geometry = new Geometry(new List<Autd3> { new Autd3(Vector3.Zero) });

        using var client = await Client.OpenAsync(geometry, new EtherCrabLinkOption(), new ClientConfig());

        Console.WriteLine($"devices: {client.NumDevices}");

        var center = geometry.Center + new Vector3(0f, 0f, 150f);
        var points = new List<ControlPoints>();
        Stm.Circle(center, 30f * mm, 200, new Vector3(0f, 0f, 1f), Intensity.Max, points);

        using var builder = client.DatagramBuilder();
        builder
            .Push(new SetSilencer())
            .Push(new FociStm(1 * Hz, points.ToArray()));
        using var frames = builder.Build();
        foreach (var frame in frames)
        {
            await client.SendCheckedAsync(frame);
        }

        Console.WriteLine("sweeping a focus around a 30 mm circle at 1 Hz — press Ctrl+C to stop");

        var stop = new TaskCompletionSource();
        Console.CancelKeyPress += (_, e) =>
        {
            e.Cancel = true;
            stop.TrySetResult();
        };
        await stop.Task;

        await client.StopAsync();
        await client.CloseAsync();
    }
}
