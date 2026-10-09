// Watch the link status for every device.
// Run with: cargo xtask cs example StatusCheck

using System;
using System.Collections.Generic;
using System.Numerics;
using System.Threading;
using System.Threading.Tasks;
using AUTD3;

internal static class Program
{
    private static readonly TimeSpan CheckInterval = TimeSpan.FromMilliseconds(100);

    private static async Task Main()
    {
        using var logGuard = Tracing.Init(new TracingOption());

        using var geometry = new Geometry(new List<Autd3> { new Autd3(Vector3.Zero) });
        await using var client = await Client.OpenAsync(geometry, new TransportOption(), new ClientConfig());
        using var checker = client.StateChecker();

        Console.WriteLine("watching link status — press Ctrl+C to stop");
        using var cts = new CancellationTokenSource();
        Console.CancelKeyPress += (_, e) =>
        {
            e.Cancel = true;
            cts.Cancel();
        };

        DeviceStatus? last = null;
        while (!cts.IsCancellationRequested)
        {
            var status = checker.Check();
            if (status != last)
            {
                for (var i = 0; i < status.Devices.Count; i++)
                {
                    Console.WriteLine($"device[{i}]: {status.Devices[i]}");
                }
                Console.WriteLine($"all ready: {status.AllReady}, any lost: {status.AnyLost}");
                last = status;
            }

            try
            {
                await Task.Delay(CheckInterval, cts.Token);
            }
            catch (TaskCanceledException)
            {
            }
        }
    }
}
