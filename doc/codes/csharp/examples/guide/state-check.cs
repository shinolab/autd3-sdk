using System;
using System.Linq;
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
        var client = Client.Open(geometry, new TransportOption());
        // ANCHOR_END: open

        using (client)
        {
            // ANCHOR: poll
            DeviceStatus[] last = Array.Empty<DeviceStatus>();
            while (true)
            {
                var status = client.DeviceStatus();
                if (!status.SequenceEqual(last))
                {
                    for (var i = 0; i < status.Length; i++)
                    {
                        Console.WriteLine($"device[{i}]: {status[i].State}");
                    }
                    var allReady = status.All(s => s.State == DeviceState.Ready);
                    var anyLost = status.Any(s => s.State == DeviceState.Lost);
                    Console.WriteLine($"all ready: {allReady}, any lost: {anyLost}");
                    last = status;
                }
                await Task.Delay(CheckInterval);
            }
            // ANCHOR_END: poll
        }
    }
}
