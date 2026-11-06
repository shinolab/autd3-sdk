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

        // ANCHOR: exec_at
        using var frames = Frames.Encode(client.Geometry, new Nop());

        var lead = ExecAt.Lead(TimeSpan.FromMilliseconds(1));
        foreach (var frame in frames)
        {
            var msgId = await client.SendFrameAsync(frame, lead);
            (await client.CollectAsync(msgId)).Check();
        }

        var at = client.DeviceTimeNow() + TimeSpan.FromMilliseconds(5);
        foreach (var frame in frames)
        {
            var msgId = await client.SendFrameAsync(frame, ExecAt.At(at));
            (await client.CollectAsync(msgId)).Check();
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

        var heartbeat = await client.CollectAsync(client.SendHeartbeat());
        client.SendHeartbeat();
        var reply = await client.RecvAsync();

        var firmware = await client.ReadFirmwareVersionAsync();
        var fpgaState = heartbeat.Replies.Select(r => r.FpgaState).ToArray();

        await client.SendAsync(new Nop());
        var sent = await client.SendFrameAsync(frames[0], ExecAt.Immediate);
        using var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(200));
        var resp = await client.CollectAsync(sent, timeout.Token);
        // ANCHOR_END: api

        _ = (numDevices, geometry, heartbeat, reply, firmware, fpgaState, resp);

        var scopedLayout = new Geometry(new[] { new Autd3() });

        // ANCHOR: context_manager
        using (var scoped = Client.Open(scopedLayout, new TransportOption()))
        {
            await scoped.SendAsync(new Nop());
        }
        // ANCHOR_END: context_manager
    }
}
