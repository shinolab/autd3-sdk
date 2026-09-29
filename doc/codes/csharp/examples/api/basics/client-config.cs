using System;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;

namespace DocSamples.ApiBasicsClientConfig;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        var udp = new TransportOption();
        var option =
            // ANCHOR: config
            new ClientConfig
            {
                AckTimeout = TimeSpan.FromMilliseconds(10),
                MaxInflight = 7,
                MaxResyncRounds = 8,
                LowLatency = false,
                RtPriority = new RtPriority(80),
                RtPolicy = RtSchedulePolicy.Fifo,
                RtAffinity = null,
                ValidateState = true,
                RequireSupportedFirmware = false,
            }
            // ANCHOR_END: config
            ;
        // ANCHOR: api
        await Client.OpenAsync(geometry, udp, option);
        // ANCHOR_END: api
    }
}
