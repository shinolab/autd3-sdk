// Watch the EtherCAT link status for every device.
// Run with: cargo xtask cs example StatusCheck

using System;
using System.Collections.Generic;
using System.Numerics;
using System.Threading;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;

internal static class Program
{
    private static readonly TimeSpan CheckInterval = TimeSpan.FromMilliseconds(100);

    private static async Task Main()
    {
        using var geometry = new Geometry(new List<Autd3> { new Autd3(Vector3.Zero) });
        var (client, checker) = await Client.OpenWithCheckerAsync(geometry, new EchocatLinkOption(), new ClientConfig());
        using var _client = client;
        using var _checker = checker;

        Console.WriteLine("watching link status — press Ctrl+C to stop");
        using var cts = new CancellationTokenSource();
        Console.CancelKeyPress += (_, e) =>
        {
            e.Cancel = true;
            cts.Cancel();
        };

        string? last = null;
        while (!cts.IsCancellationRequested)
        {
            var status = checker.Check();
            var key = string.Join(",", status.Devices) + $"|{status.Recoveries}";
            if (key != last)
            {
                for (var i = 0; i < status.Devices.Count; i++)
                {
                    Console.WriteLine($"device[{i}]: {status.Devices[i]}");
                }
                Console.WriteLine($"all operational: {status.AllOp}, any lost: {status.AnyLost}, recoveries: {status.Recoveries}");
                last = key;
            }

            try
            {
                await Task.Delay(CheckInterval, cts.Token);
            }
            catch (TaskCanceledException)
            {
            }
        }

        await client.CloseAsync();
    }
}
