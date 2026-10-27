using System;
using System.Linq;
using System.Threading.Tasks;
using AUTD3;

namespace DocSamples.GuideFailsafe;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        var client = Client.Open(geometry, new TransportOption());

        using (client)
        {
            // ANCHOR: ptp_failsafe
            var status = client.DeviceStatus();
            static bool Releasable(DeviceStatus s) => s.FailsafeActive && s.PtpLocked;
            if (status.Any(Releasable))
            {
                await client.SendAsync(Command.Each(device => Releasable(status[device.Idx]) ? new ReleaseFailsafe() : null));
            }
            // ANCHOR_END: ptp_failsafe

            // ANCHOR: heartbeat
            await client.SendAsync(new SetCpuConfig(new CpuConfig { FailsafeTimeout = TimeSpan.FromMilliseconds(100) }));

            while (true)
            {
                client.SendHeartbeat();
                await Task.Delay(TimeSpan.FromMilliseconds(10));
            }
            // ANCHOR_END: heartbeat

        }
    }
}
