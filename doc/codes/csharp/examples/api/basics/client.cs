using System.Numerics;
using System.Threading.Tasks;
using AUTD3;

namespace DocSamples.ApiBasicsClient;

internal static class Sample
{
    internal static async Task Run()
    {
        var layout = new Geometry(new[] { new Autd3(Vector3.Zero) });
        using var emulator = new UdpEmulator(layout.NumDevices);
        var client = await Client.OpenAsync(layout, emulator.Option(), new ClientConfig());

        using var frames = Frames.Encode(layout, new Nop());
        var frame = frames[0];

        // ANCHOR: api
        var numDevices = client.NumDevices;
        var geometry = client.Geometry;

        var firmware = await client.ReadFirmwareVersionAsync();
        var fpgaState = await client.ReadFpgaStateAsync();

        await client.SendAsync(new Nop());
        var done = await client.SendStreamingAsync(new Nop());
        var resp = await await client.SendFrameAsync(frame);

        await client.SilentStopAsync();
        await client.CloseAsync();
        // ANCHOR_END: api

        _ = (numDevices, geometry, firmware, fpgaState, done, resp);

        var scopedLayout = new Geometry(new[] { new Autd3(Vector3.Zero) });

        // ANCHOR: context_manager
        using var scopedEmulator = new UdpEmulator(scopedLayout.NumDevices);
        await using (var scoped = await Client.OpenAsync(scopedLayout, scopedEmulator.Option(), new ClientConfig()))
        {
            await scoped.SendAsync(new Nop());
        }
        // ANCHOR_END: context_manager
    }
}
