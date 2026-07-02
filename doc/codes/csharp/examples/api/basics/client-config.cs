using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;

namespace AUTD3.DocSamples.ApiBasicsClientConfig;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

        var link = NopLink.Create();
        var option =
            // ANCHOR: config
            new ClientConfig(
                lowLatency: false,
                timeoutCycles: 10,
                maxInflight: (uint)Client.MaxInflight,
                sendIntervalCycles: 1,
                maxResyncRounds: 8,
                resetResendCycles: 2,
                rtPriority: null,
                rtAffinity: null,
                validateState: true
            )
            // ANCHOR_END: config
            ;
        // ANCHOR: api
        await Client.OpenAsync(geometry, link, option);
        // ANCHOR_END: api
    }
}
