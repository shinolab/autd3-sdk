// A focus moving along a circle at 1 Hz using FociSTM.
// Run with: cargo xtask cs example StmCircle

using System;
using System.Collections.Generic;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;

internal static class Program
{
    private static async Task Main()
    {
        using var geometry = new Geometry(new List<Device> { new Device(Vector3.Zero) });

        using var client = await Client.OpenAsync(geometry, EtherCrabLink.Create(), new ClientConfig());

        Console.WriteLine($"devices: {client.NumDevices}");

        var center = geometry.Center + new Vector3(0f, 0f, 150f);
        var points = Stm.Circle(center, 30f, 50, new Vector3(0f, 0f, 1f));

        using var builder = client.DatagramBuilder();
        builder.Push(new FociStm(StmConfig.Freq(1f), points));
        using var datagrams = builder.Build();
        foreach (var frame in datagrams)
        {
            await client.SendCheckedAsync(frame);
        }

        Console.WriteLine("emitting a 1 Hz circular FociSTM (50 points) — press Ctrl+C to stop");

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
