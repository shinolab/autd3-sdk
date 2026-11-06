using System;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using AUTD3;

namespace DocSamples.ApiBasicsClient;

internal static class Sample
{
    internal static async Task Run()
    {
        var layout = new Geometry(new[] { new Autd3() });
        // ANCHOR: open
        var client = Client.Open(layout, new TransportOption());
        await client.SendAsync(new Clear());
        await client.SendAsync(new Synchronize());
        // ANCHOR_END: open

        // ANCHOR: api
        var numDevices = client.NumDevices;
        var now = client.DeviceTimeNow();

        var firmware = await client.ReadFirmwareVersionAsync();

        await client.SendAsync(new Nop());
        await client.SendStreamingAsync(new Nop());

        using var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(200));
        var state = await client.ReadStateAsync(timeout.Token);
        var fpgaState = state.Replies.Select(r => r.FpgaState).ToArray();
        // ANCHOR_END: api

        _ = (numDevices, now, firmware, fpgaState);

        var scopedLayout = new Geometry(new[] { new Autd3() });

        // ANCHOR: context_manager
        using (var scoped = Client.Open(scopedLayout, new TransportOption()))
        {
            await scoped.SendAsync(new Nop());
        }
        // ANCHOR_END: context_manager
    }
}
