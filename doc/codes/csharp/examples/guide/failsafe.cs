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

        // ANCHOR: heartbeat
        var client = Client.Open(geometry, new TransportOption { HeartbeatInterval = TimeSpan.FromMilliseconds(10) });

        await client.SendAsync(new SetCpuConfig(new CpuConfig { FailsafeTimeout = TimeSpan.FromMilliseconds(100) }));
        // ANCHOR_END: heartbeat

        using (client)
        {
            // ANCHOR: ptp_failsafe
            var status = (await client.ReadStateAsync()).DeviceStatus();
            static bool Releasable(DeviceStatus s) => s.FailsafeActive && s.PtpLocked;
            if (status.Any(Releasable))
            {
                await client.SendAsync(Command.Each(device => Releasable(status[device.Idx]) ? new ReleaseFailsafe() : null));
            }
            // ANCHOR_END: ptp_failsafe
        }
    }
}
