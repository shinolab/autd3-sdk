// Watch the EtherCAT link status for every device.
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
        using var geometry = new Geometry(new List<Autd3> { new Autd3(Vector3.Zero) });
        var (driver, connector) = Driver.Open(new TransportOption(), geometry.NumDevices);
        using var _driver = driver;
        using var checker = driver.StateChecker();
        new Thread(driver.Run) { IsBackground = true }.Start();
        await using var client = await Client.OpenAsync(geometry, connector, new ClientConfig());

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
            var key = string.Join(",", status.Devices);
            if (key != last)
            {
                for (var i = 0; i < status.Devices.Count; i++)
                {
                    Console.WriteLine($"device[{i}]: {status.Devices[i]}");
                }
                Console.WriteLine($"all ready: {status.AllReady}, any lost: {status.AnyLost}");
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
    }
}
