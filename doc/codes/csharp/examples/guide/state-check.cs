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
        using var checker = client.StateChecker();
        // ANCHOR_END: open

        using (client)
        {
            // ANCHOR: ptp_failsafe
            client.SendHeartbeat();
            await Task.Delay(CheckInterval);
            var current = checker.Check();
            if (current.AnyPtpFailsafe)
            {
                var releasable = DeviceMask.FromDevices(current.PtpFailsafe.Where(i => current.PtpLocked[i]));
                if (!releasable.IsEmpty)
                {
                    await client.SendAsync(Command.Each(device => releasable[device.Idx] ? new ReleaseFailsafe() : null));
                }
            }
            // ANCHOR_END: ptp_failsafe

            // ANCHOR: poll
            DeviceStatus? last = null;
            while (true)
            {
                client.SendHeartbeat();
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
