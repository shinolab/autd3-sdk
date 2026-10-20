using System;
using System.Threading.Tasks;
using AUTD3;

namespace DocSamples.ApiBasicsClient;

internal static class Sample
{
    internal static async Task Run()
    {
        var layout = new Geometry(new[] { new Autd3() });
        var client = await Client.OpenAsync(layout, new UdpOption(), new ClientConfig());

        using var frames = Frames.Encode(layout, new Nop());
        var frame = frames[0];

        // ANCHOR: exec_at
        var at = client.DeviceTimeNow() + TimeSpan.FromMilliseconds(5);
        (await await client.SendFrameAsync(frame, ExecAt.At(at))).Check();

        var lead = ExecAt.Lead(TimeSpan.FromMilliseconds(1));
        (await await client.SendFrameAsync(frame, lead)).Check();

        (await await client.SendFrameAsync(frame, ExecAt.Immediate)).Check();
        // ANCHOR_END: exec_at

        // ANCHOR: api
        var numDevices = client.NumDevices;
        var geometry = client.Geometry;

        var config = client.Config;
        client.SetConfig(new ClientConfig { AckTimeout = TimeSpan.FromMilliseconds(20) });

        var firmware = await client.ReadFirmwareVersionAsync();
        var fpgaState = await client.ReadFpgaStateAsync();

        await client.SendAsync(new Nop());
        var done = await client.SendStreamingAsync(new Nop());
        var resp = await await client.SendFrameAsync(frame);

        await client.SilentStopAsync();
        await client.CloseAsync();
        // ANCHOR_END: api

        _ = (numDevices, geometry, config, firmware, fpgaState, done, resp);

        var scopedLayout = new Geometry(new[] { new Autd3() });

        // ANCHOR: context_manager
        await using (var scoped = await Client.OpenAsync(scopedLayout, new UdpOption(), new ClientConfig()))
        {
            await scoped.SendAsync(new Nop());
        }
        // ANCHOR_END: context_manager
    }
}
