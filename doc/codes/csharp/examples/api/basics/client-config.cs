using System;
using System.Threading.Tasks;
using AUTD3;

namespace DocSamples.ApiBasicsClientConfig;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        var udp = new TransportOption();
        var option =
            // ANCHOR: config
            new ClientConfig
            {
                AckTimeout = TimeSpan.FromMilliseconds(10),
                MaxInflight = (uint)Params.DeviceQueueFrames,
            }
            // ANCHOR_END: config
            ;
        // ANCHOR: api
        var client = await Client.OpenAsync(geometry, udp, option);
        await client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));
        // ANCHOR_END: api
    }
}
