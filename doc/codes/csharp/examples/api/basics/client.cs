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

        var frames = client.DatagramBuilder().Build();
        var frame = frames[0];

        // ANCHOR: api
        var numDevices = client.NumDevices;
        var geometry = client.Geometry;

        var firmware = await client.ReadFirmwareVersionAsync();
        var fpgaState = await client.ReadFpgaStateAsync();
        var errorDetail = await client.ReadErrorDetailAsync();

        var datagramBuilder = client.DatagramBuilder();
        var resp = await await client.SendAsync(frame);
        await client.SendCheckedAsync(frame);

        await client.StopAsync();
        await client.CloseAsync();
        // ANCHOR_END: api

        _ = (numDevices, geometry, firmware, fpgaState, errorDetail, datagramBuilder);

        var scopedLayout = new Geometry(new[] { new Autd3(Vector3.Zero) });

        // ANCHOR: context_manager
        using var scopedEmulator = new UdpEmulator(scopedLayout.NumDevices);
        await using (var scoped = await Client.OpenAsync(scopedLayout, scopedEmulator.Option(), new ClientConfig()))
        {
            await scoped.SendCheckedAsync(frame);
        }
        // ANCHOR_END: context_manager
    }
}
