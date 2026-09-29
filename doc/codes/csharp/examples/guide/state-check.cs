using System;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;

namespace DocSamples.GuideStateCheck;

internal static class Sample
{
    private static readonly TimeSpan CheckInterval = TimeSpan.FromMilliseconds(100);

    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        // ANCHOR: open
        var (client, checker) = await Client.OpenWithCheckerAsync(
            geometry,
            new TransportOption(),
            new ClientConfig()
        );
        // ANCHOR_END: open

        await using (client)
        {
            // ANCHOR: poll
            DeviceStatus? last = null;
            while (true)
            {
                var status = checker.Check();
                if (status != last)
                {
                    for (var i = 0; i < status.Devices.Count; i++)
                    {
                        Console.WriteLine($"device[{i}]: {status.Devices[i]}");
                    }
                    Console.WriteLine($"all operational: {status.AllOp}, any lost: {status.AnyLost}, recoveries: {status.Recoveries}");
                    last = status;
                }
                await Task.Delay(CheckInterval);
            }
            // ANCHOR_END: poll
        }
    }
}
