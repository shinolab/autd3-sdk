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
            new ClientConfig
            {
                TimeoutCycles = 10,
                MaxInflight = (uint)Client.MaxInflight,
                MaxResyncRounds = 8,
                LowLatency = false,
                ResetResendCycles = 2,
                RtPriority = new RtPriority(80),
                RtPolicy = RtSchedulePolicy.Fifo,
                RtAffinity = null,
                ValidateState = true,
                RequireSupportedFirmware = false,
            }
            // ANCHOR_END: config
            ;
        // ANCHOR: api
        await Client.OpenAsync(geometry, link, option);
        // ANCHOR_END: api
    }
}
