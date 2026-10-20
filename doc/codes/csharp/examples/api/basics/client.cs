using System;
using System.Threading.Tasks;
using AUTD3;

namespace DocSamples.ApiBasicsClient;

internal static class Sample
{
    internal static async Task Run()
    {
        var layout = new Geometry(new[] { new Autd3() });
        // ANCHOR: open
        var client = await Client.OpenAsync(layout, new TransportOption());
        await client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));
        // ANCHOR_END: open

        // ANCHOR: exec_at
        using var frames = Frames.Encode(client.Geometry, new Nop());

        var lead = ExecAt.Lead(TimeSpan.FromMilliseconds(1));
        foreach (var frame in frames)
        {
            (await await client.SendFrameAsync(frame, lead)).Check();
        }

        var at = client.DeviceTimeNow() + TimeSpan.FromMilliseconds(5);
        foreach (var frame in frames)
        {
            (await await client.SendFrameAsync(frame, ExecAt.At(at))).Check();
        }
        // ANCHOR_END: exec_at

        // ANCHOR: heartbeat
        for (var i = 0; i < 100; i++)
        {
            client.SendHeartbeat();
            await Task.Delay(TimeSpan.FromMilliseconds(10));
        }
        // ANCHOR_END: heartbeat

        // ANCHOR: api
        var numDevices = client.NumDevices;
        var geometry = client.Geometry;

        client.SendHeartbeat();

        var firmware = await client.ReadFirmwareVersionAsync();
        var fpgaState = await client.ReadFpgaStateAsync();

        await client.SendAsync(new Nop());
        var resp = await await client.SendFrameAsync(frames[0]);

        await client.CloseAsync();
        // ANCHOR_END: api

        _ = (numDevices, geometry, firmware, fpgaState, resp);

        var scopedLayout = new Geometry(new[] { new Autd3() });

        // ANCHOR: context_manager
        await using (var scoped = await Client.OpenAsync(scopedLayout, new TransportOption()))
        {
            await scoped.SendAsync(new Nop());
        }
        // ANCHOR_END: context_manager
    }
}
