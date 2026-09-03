using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using Nop = AUTD3.Link.Nop;

namespace DocSamples.ApiBasicsClient;

internal static class Sample
{
    internal static async Task Run()
    {
        var layout = new Geometry(new[] { new Autd3(Vector3.Zero) });
        var client = await Client.OpenAsync(layout, new Nop(), new ClientConfig());

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
    }
}
