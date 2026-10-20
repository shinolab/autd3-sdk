using System;
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
        using var checker = client.StateChecker();
        // ANCHOR_END: open

        await using (client)
        {
            // ANCHOR: recover
            if (checker.Check().AnyLost)
            {
                var recovered = await client.RecoverAsync();
                if (!recovered.IsEmpty)
                {
                    await client.SendAsync(Command.Each(device => recovered[device.Idx] ? new Synchronize() : null));
                    await client.SendAsync(new ReleaseFailsafe());
                }
            }
            // ANCHOR_END: recover

            // ANCHOR: poll
            DeviceStatus? last = null;
            while (true)
            {
                client.Heartbeat();
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
                await Task.Delay(CheckInterval);
            }
            // ANCHOR_END: poll
        }
    }
}
