using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using Nop = AUTD3.Link.Nop;

namespace DocSamples.ApiBasicsClientConfig;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        var link = new Nop();
        var option =
            // ANCHOR: config
            new ClientConfig(
                timeoutCycles: 10,
                maxInflight: (uint)Client.MaxInflight,
                maxResyncRounds: 8,
                lowLatency: false,
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
