using System;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;

namespace DocSamples.GuideStateCheck;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        // ANCHOR: open
        var (client, checker) = await Client.OpenWithCheckerAsync(
            geometry,
            new EtherCrabLinkOption(),
            new ClientConfig()
        );
        // ANCHOR_END: open

        try
        {
            // ANCHOR: poll
            LinkStatus? last = null;
            while (true)
            {
                var status = await checker.CheckAsync();
                if (status != last)
                {
                    for (var i = 0; i < status.Devices.Count; i++)
                    {
                        Console.WriteLine($"device[{i}]: {status.Devices[i]}");
                    }
                    Console.WriteLine($"all operational: {status.AllOp}, any lost: {status.AnyLost}, recoveries: {status.Recoveries}");
                    last = status;
                }
                await Task.Delay(TimeSpan.FromMilliseconds(100));
            }
            // ANCHOR_END: poll
        }
        finally
        {
            await client.CloseAsync();
        }
    }
}
