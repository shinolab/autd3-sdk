using System;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using AUTD3;

namespace DocSamples.GuideStateCheck;

internal static class Sample
{
    private static readonly TimeSpan CheckInterval = TimeSpan.FromMilliseconds(100);

    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        // ANCHOR: open
        var client = await Client.OpenAsync(geometry, new TransportOption());
        // ANCHOR_END: open

        using (client)
        {
            // ANCHOR: poll
            DeviceStatus[] last = Array.Empty<DeviceStatus>();
            while (true)
            {
                using var timeout = new CancellationTokenSource(CheckInterval);
                DeviceStatus[] status;
                try
                {
                    status = (await client.ReadStateAsync(timeout.Token)).DeviceStatus();
                }
                catch (OperationCanceledException)
                {
                    Console.WriteLine($"not every device replied within {CheckInterval}");
                    continue;
                }
                if (!status.SequenceEqual(last))
                {
                    for (var i = 0; i < status.Length; i++)
                    {
                        Console.WriteLine($"device[{i}]: {status[i].State}");
                    }
                    var allReady = status.All(s => s.State == DeviceState.Ready);
                    Console.WriteLine($"all ready: {allReady}");
                    last = status;
                }
                await Task.Delay(CheckInterval);
            }
            // ANCHOR_END: poll
        }
    }
}
