using System;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using AUTD3;

namespace DocSamples.GuideFailsafe;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        // ANCHOR: heartbeat
        var client = Client.Open(geometry, new TransportOption());
        using var stopHeartbeat = new CancellationTokenSource();
        var heartbeat = Task.Run(async () =>
        {
            while (!stopHeartbeat.IsCancellationRequested)
            {
                try
                {
                    client.SendHeartbeat();
                }
                catch (Autd3Exception e)
                {
                    Console.WriteLine($"sending a heartbeat failed: {e.Message}");
                }
                await Task.Delay(TimeSpan.FromMilliseconds(10));
            }
        });

        await client.SendAsync(new SetCpuConfig(new CpuConfig { FailsafeTimeout = TimeSpan.FromMilliseconds(100) }));
        // ANCHOR_END: heartbeat

        using (client)
        {
            // ANCHOR: ptp_failsafe
            var status = (await client.CollectAsync(client.SendHeartbeat())).DeviceStatus();
            static bool Releasable(DeviceStatus s) => s.FailsafeActive && s.PtpLocked;
            if (status.Any(Releasable))
            {
                await client.SendAsync(Command.Each(device => Releasable(status[device.Idx]) ? new ReleaseFailsafe() : null));
            }
            // ANCHOR_END: ptp_failsafe

            stopHeartbeat.Cancel();
            await heartbeat;
        }
    }
}
